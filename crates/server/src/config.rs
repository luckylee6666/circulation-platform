use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::Context;

/// 运行期配置。桌面端会把数据目录指到用户目录，独立运行时默认落在项目下的 `./data`。
#[derive(Debug, Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    /// 会话有效期（小时）
    pub session_ttl_hours: i64,
    /// 前端开发模式：静态资源改从磁盘读取，便于 Vite 热更
    pub dev_static_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8080,
            data_dir: PathBuf::from("data"),
            session_ttl_hours: 12 * 7,
            dev_static_dir: None,
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut config = Config::default();

        if let Ok(host) = std::env::var("CIRCULATION_HOST") {
            config.host = host.parse().with_context(|| format!("CIRCULATION_HOST 不是合法 IP: {host}"))?;
        }
        if let Ok(port) = std::env::var("CIRCULATION_PORT") {
            config.port = port.parse().with_context(|| format!("CIRCULATION_PORT 不是合法端口: {port}"))?;
        }
        if let Ok(dir) = std::env::var("CIRCULATION_DATA_DIR") {
            config.data_dir = PathBuf::from(dir);
        }
        if let Ok(dir) = std::env::var("CIRCULATION_DEV_STATIC_DIR") {
            config.dev_static_dir = Some(PathBuf::from(dir));
        }
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("data").join("circulation.db")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        let dirs = [
            self.db_path().parent().map(Path::to_path_buf),
            Some(self.log_dir()),
            Some(self.backup_dir()),
        ];

        for dir in dirs.into_iter().flatten() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("无法创建目录 {}", dir.display()))?;
        }
        Ok(())
    }
}

/// 初始化日志：同时输出到标准输出与 `logs/app.log`（按天切分）。
///
/// 返回的 guard 必须一直持有，否则后台写日志线程会被提前关闭。
pub fn init_tracing(config: &Config) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    std::fs::create_dir_all(config.log_dir())?;
    let file_appender = tracing_appender::rolling::daily(config.log_dir(), "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    // 默认 info；HTTP 栈和 TLS 的日志太吵，单独压到 warn。
    // 需要排查时用 RUST_LOG 覆盖即可。
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn,rustls=warn,hyper=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .try_init()
        .map_err(|err| anyhow::anyhow!("日志初始化失败（可能已被初始化过）: {err}"))?;

    Ok(guard)
}
