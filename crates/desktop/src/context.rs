use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use circulation_server::config::Config as ServerConfig;
use circulation_server::db;
use tauri::AppHandle;

use crate::service::Service;
use crate::settings::Settings;

pub struct Paths {
    pub settings_file: PathBuf,
    pub default_data_dir: PathBuf,
}

pub struct AppContext {
    pub service: Service,
    pub paths: Paths,
    settings: Mutex<Settings>,
    pub quitting: AtomicBool,
}

impl AppContext {
    pub fn new(paths: Paths, settings: Settings) -> Self {
        let service = Service::new(settings.port);
        Self {
            service,
            paths,
            settings: Mutex::new(settings),
            quitting: AtomicBool::new(false),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.settings().resolved_data_dir(&self.paths.default_data_dir)
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir().join("backups")
    }

    /// 数据库连接池：服务在跑就复用它的池，没跑就临时开一个（例如只为备份）。
    pub fn open_pool(&self) -> Result<db::Pool, String> {
        if let Some(pool) = self.service.pool() {
            return Ok(pool);
        }
        let path = self.server_config().db_path();
        db::init_pool(&path).map_err(|err| format!("无法打开数据库：{err}"))
    }

    pub fn server_config(&self) -> ServerConfig {
        let settings = self.settings();
        ServerConfig {
            port: settings.port,
            data_dir: settings.resolved_data_dir(&self.paths.default_data_dir),
            ..ServerConfig::default()
        }
    }

    /// 落盘设置，并同步开机自启状态。
    pub fn apply_settings(&self, app: &AppHandle, settings: Settings) -> Result<(), String> {
        let autostart = settings.autostart;

        if let Ok(mut current) = self.settings.lock() {
            *current = settings.clone();
        }
        settings
            .save(&self.paths.settings_file)
            .map_err(|err| format!("保存设置失败：{err}"))?;

        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        let currently_enabled = manager.is_enabled().unwrap_or(false);

        let result = if autostart && !currently_enabled {
            manager.enable()
        } else if !autostart && currently_enabled {
            manager.disable()
        } else {
            Ok(())
        };

        result.map_err(|err| format!("设置开机自启失败：{err}"))
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    pub fn mark_quitting(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }
}
