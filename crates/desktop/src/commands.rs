use std::path::Path;
use std::sync::atomic::Ordering;

use circulation_server::auth::session;
use circulation_server::db;
use circulation_server::domain::setting;
use circulation_server::error::AppResult;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::backup::{self, BackupFile};
use crate::net::{self, NetworkAddress};
use crate::service::ServiceStatus;
use crate::settings::Settings;
use crate::AppContext;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub database_path: String,
    pub backup_dir: String,
    pub log_dir: String,
    pub settings_file: String,
}

/// 服务在跑就用它的连接池，停着就临时开一个——
/// 平台名称是本地配置，不该依赖服务是否运行。
async fn run_with_pool<T, F>(state: &AppContext, f: F) -> Result<T, String>
where
    F: FnOnce(&mut db::PooledConn) -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    let pool = match state.service.pool() {
        Some(pool) => pool,
        None => state.open_pool()?,
    };
    db::run(pool, f).await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_site_name(state: State<'_, AppContext>) -> Result<String, String> {
    let branding = run_with_pool(&state, |conn| setting::branding(conn)).await?;
    Ok(branding.name)
}

#[tauri::command]
pub async fn save_site_name(state: State<'_, AppContext>, name: String) -> Result<String, String> {
    let name = setting::normalize_site_name(&name).map_err(|err| err.to_string())?;
    let saved = name.clone();
    run_with_pool(&state, move |conn| {
        setting::set(conn, setting::KEY_SITE_NAME, &saved)?;
        Ok(())
    })
    .await?;
    Ok(name)
}

#[tauri::command]
pub async fn service_status(state: State<'_, AppContext>) -> Result<ServiceStatus, String> {
    let mut status = state.service.status(&state.data_dir());

    // 有效会话数只有库里有；这个接口 2 秒轮询一次，计数走索引，开销可以忽略
    if status.running
        && let Some(pool) = state.service.pool()
        && let Ok(count) = db::run(pool, |conn| session::active_count(conn)).await
    {
        status.online_sessions = count;
    }

    Ok(status)
}

#[tauri::command]
pub async fn service_start(
    app: AppHandle,
    state: State<'_, AppContext>,
    port: Option<u16>,
) -> Result<ServiceStatus, String> {
    if !matches!(port, None | Some(0)) {
        let mut settings = state.settings().clone();
        settings.port = port.unwrap();
        state.apply_settings(&app, settings)?;
    }

    let config = state.server_config();

    if let Err(message) = state.service.start(config).await {
        state.service.record_error(message.clone());
        return Err(message);
    }

    Ok(state.service.status(&state.data_dir()))
}

#[tauri::command]
pub fn service_stop(state: State<'_, AppContext>) -> ServiceStatus {
    state.service.stop();
    state.service.status(&state.data_dir())
}

#[tauri::command]
pub fn access_addresses(state: State<'_, AppContext>) -> Vec<NetworkAddress> {
    let port = if state.service.is_running() {
        state.service.port()
    } else {
        state.settings().port
    };
    net::lan_addresses(port)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppContext>) -> Settings {
    state.settings().clone()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppContext>,
    settings: Settings,
) -> Result<Settings, String> {
    let port_changed = settings.port != state.settings().port
        && state.service.is_running();

    state.apply_settings(&app, settings.clone())?;

    // 端口是在启动时绑定的，改了端口必须重启服务才生效
    if port_changed {
        state.service.stop();
    }

    Ok(settings)
}

#[tauri::command]
pub async fn backup_now(state: State<'_, AppContext>) -> Result<BackupFile, String> {
    let pool = state.open_pool()?;
    let result = backup::create(pool, state.backup_dir()).await?;
    Ok(result.backup)
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppContext>) -> Result<Vec<BackupFile>, String> {
    backup::list(&state.backup_dir())
}

#[tauri::command]
pub fn recent_logs(state: State<'_, AppContext>, lines: Option<usize>) -> Result<String, String> {
    read_recent_logs(&state.data_dir().join("logs"), lines.unwrap_or(200))
}

#[tauri::command]
pub fn app_info(state: State<'_, AppContext>) -> AppInfo {
    let data_dir = state.data_dir();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        database_path: data_dir.join("data").join("circulation.db").display().to_string(),
        backup_dir: data_dir.join("backups").display().to_string(),
        log_dir: data_dir.join("logs").display().to_string(),
        settings_file: state.paths.settings_file.display().to_string(),
        data_dir: data_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle, state: State<'_, AppContext>) {
    state.quitting.store(true, Ordering::SeqCst);
    state.service.stop();
    app.exit(0);
}

/// 剪贴板、打开链接等都收在 Rust 侧，
/// 这样前端只需要拿 `core:default` 权限，不用为每个插件再开一套 capability。
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(text)
        .map_err(|err| format!("写入剪贴板失败：{err}"))
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| format!("打开链接失败：{err}"))
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|err| format!("打开目录失败：{err}"))
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|err| format!("定位文件失败：{err}"))
}

/// 读取最新日志文件的末尾若干行，供控制台直接查看。
pub fn read_recent_logs(log_dir: &Path, lines: usize) -> Result<String, String> {
    let entries = std::fs::read_dir(log_dir).map_err(|err| format!("无法读取日志目录：{err}"))?;

    let newest = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("app.log"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);

    let Some(path) = newest else {
        return Ok("暂无日志".to_string());
    };

    let content = std::fs::read_to_string(&path).map_err(|err| format!("无法读取日志文件：{err}"))?;
    let tail: Vec<&str> = content.lines().rev().take(lines).collect();

    Ok(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
}
