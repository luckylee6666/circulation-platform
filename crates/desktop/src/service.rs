use std::net::SocketAddr;
use std::path::Path;
use std::sync::Mutex;

use circulation_server::config::Config as ServerConfig;
use circulation_server::db;
use circulation_server::state::ActivityTracker;
use circulation_server::{bootstrap, build_app};
use serde::Serialize;

use crate::load::{CurrentLoad, LoadPoint, LoadSampler};

/// 多长时间内发过请求算「在线」
const ONLINE_WINDOW: std::time::Duration = std::time::Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub running: bool,
    pub port: u16,
    pub started_at: Option<String>,
    pub uptime_seconds: i64,
    pub data_dir: String,
    pub database_size: u64,
    pub last_error: Option<String>,
    /// 服务进程当前占用的内存（字节）
    pub memory_bytes: u64,
    /// 服务进程 CPU 占用百分比
    pub cpu_percent: f32,
    /// 最近几分钟有实际操作的用户数
    pub online_users: usize,
    /// 还有效的登录凭证数（不代表人在电脑前）
    pub online_sessions: i64,
    /// 内存占用历史，用来观察长期运行有没有持续增长
    pub load_history: Vec<LoadPoint>,
}

pub struct Service {
    inner: Mutex<Inner>,
    load: Mutex<LoadSampler>,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    last_error: Option<String>,
    last_port: u16,
}

struct Running {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    port: u16,
    started_at: chrono::DateTime<chrono::Local>,
    pool: db::Pool,
    activity: std::sync::Arc<ActivityTracker>,
}

impl Service {
    pub fn new(port: u16) -> Self {
        Self {
            inner: Mutex::new(Inner {
                last_port: port,
                ..Inner::default()
            }),
            load: Mutex::new(LoadSampler::new()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.running.is_some())
            .unwrap_or(false)
    }

    pub fn port(&self) -> u16 {
        self.inner
            .lock()
            .map(|inner| inner.running.as_ref().map_or(inner.last_port, |run| run.port))
            .unwrap_or(0)
    }

    /// 启动内嵌服务。端口被占用一类的错误会转成能直接显示给用户的中文提示。
    pub async fn start(&self, config: ServerConfig) -> Result<(), String> {
        {
            let inner = self.lock()?;
            if inner.running.is_some() {
                return Err("服务已经在运行".to_string());
            }
        }

        let port = config.port;
        let data_dir = config.data_dir.clone();

        // 先抢占端口，让「端口被占用」在启动数据库之前就暴露出来
        let listener = tokio::net::TcpListener::bind(config.bind_addr())
            .await
            .map_err(|err| describe_bind_error(port, &err))?;

        let state = tokio::task::spawn_blocking(move || bootstrap(config))
            .await
            .map_err(|err| format!("初始化数据失败：{err}"))?
            .map_err(|err| format!("初始化数据失败：{err}"))?;

        let pool = state.pool.clone();
        let activity = state.activity.clone();
        let app = build_app(state).map_err(|err| format!("构建服务失败：{err}"))?;

        let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();

        tauri::async_runtime::spawn(async move {
            let served = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = receiver.await;
            })
            .await;

            match served {
                Ok(()) => tracing::info!("服务已停止"),
                Err(err) => tracing::error!("服务异常退出：{err}"),
            }
        });

        {
            let mut inner = self.lock()?;
            inner.running = Some(Running {
                shutdown: Some(shutdown),
                port,
                started_at: chrono::Local::now(),
                pool,
                activity,
            });
            inner.last_port = port;
            inner.last_error = None;
        }

        tracing::info!("服务已启动，监听 0.0.0.0:{port}，数据目录 {}", data_dir.display());
        Ok(())
    }

    /// 通知服务优雅退出。返回是否真的发出了停止信号。
    pub fn stop(&self) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };

        match inner.running.take() {
            Some(mut running) => {
                if let Some(sender) = running.shutdown.take() {
                    let _ = sender.send(());
                }
                true
            }
            None => false,
        }
    }

    pub fn record_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_error = Some(message);
        }
    }

    /// 服务运行期间对外暴露连接池，避免为备份等操作重复打开数据库。
    pub fn pool(&self) -> Option<db::Pool> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.running.as_ref().map(|running| running.pool.clone()))
    }

    pub fn status(&self, data_dir: &Path) -> ServiceStatus {
        // 负载先采，服务停着也能看到桌面端自身占了多少
        let load = self.sample_load();
        let history = self
            .load
            .lock()
            .map(|sampler| sampler.history())
            .unwrap_or_default();

        let Ok(inner) = self.inner.lock() else {
            return ServiceStatus {
                data_dir: data_dir.display().to_string(),
                memory_bytes: load.memory_bytes,
                cpu_percent: load.cpu_percent,
                load_history: history,
                ..ServiceStatus::default()
            };
        };

        let base = ServiceStatus {
            data_dir: data_dir.display().to_string(),
            database_size: database_size(data_dir),
            last_error: inner.last_error.clone(),
            memory_bytes: load.memory_bytes,
            cpu_percent: load.cpu_percent,
            load_history: history,
            ..ServiceStatus::default()
        };

        match inner.running.as_ref() {
            Some(running) => ServiceStatus {
                running: true,
                port: running.port,
                started_at: Some(running.started_at.format("%Y-%m-%d %H:%M:%S").to_string()),
                uptime_seconds: (chrono::Local::now() - running.started_at).num_seconds().max(0),
                online_users: running.activity.online_users(ONLINE_WINDOW),
                ..base
            },
            None => ServiceStatus {
                port: inner.last_port,
                ..base
            },
        }
    }

    fn sample_load(&self) -> CurrentLoad {
        self.load
            .lock()
            .map(|mut sampler| sampler.sample())
            .unwrap_or_default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, String> {
        self.inner.lock().map_err(|_| "内部状态异常，请重启应用".to_string())
    }
}

/// SQLite 的库、WAL、共享内存三个文件一起算，才是真实占用。
pub fn database_size(data_dir: &Path) -> u64 {
    let db = data_dir.join("data").join("circulation.db");
    ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| {
            let path = if suffix.is_empty() {
                db.clone()
            } else {
                Path::new(&format!("{}{suffix}", db.display())).to_path_buf()
            };
            std::fs::metadata(path).ok().map(|meta| meta.len())
        })
        .sum()
}

fn describe_bind_error(port: u16, err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("端口 {port} 已被其它程序占用，请在设置里换一个端口")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("没有权限监听端口 {port}，请改用 1024 以上的端口")
        }
        _ => format!("无法在端口 {port} 启动服务：{err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_stopped_before_start() {
        let service = Service::new(8080);
        let dir = tempfile::tempdir().unwrap();

        let status = service.status(dir.path());
        assert!(!status.running);
        assert_eq!(status.port, 8080);
        assert_eq!(status.uptime_seconds, 0);
        assert!(status.started_at.is_none());
    }

    #[test]
    fn stop_on_stopped_service_is_noop() {
        let service = Service::new(8080);
        assert!(!service.stop());
        assert!(!service.is_running());
    }

    #[test]
    fn database_size_counts_sidecar_files() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        assert_eq!(database_size(dir.path()), 0);

        std::fs::write(data.join("circulation.db"), vec![0u8; 100]).unwrap();
        std::fs::write(data.join("circulation.db-wal"), vec![0u8; 50]).unwrap();

        assert_eq!(database_size(dir.path()), 150);
    }

    #[test]
    fn bind_errors_are_explained_in_plain_language() {
        let in_use = std::io::Error::from(std::io::ErrorKind::AddrInUse);
        let message = describe_bind_error(8080, &in_use);
        assert!(message.contains("8080"));
        assert!(message.contains("占用"));

        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(describe_bind_error(80, &denied).contains("权限"));
    }
}
