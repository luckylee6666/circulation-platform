pub mod backup;
pub mod commands;
pub mod load;
pub mod context;
pub mod net;
pub mod service;
pub mod settings;
pub mod tray;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use tauri::{Manager, WindowEvent};

pub use context::{AppContext, Paths};

/// 日志后台写线程的守卫，必须活到进程结束，否则退出时会丢日志。
static TRACING_GUARD: OnceLock<Mutex<tracing_appender::non_blocking::WorkerGuard>> = OnceLock::new();

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 重复双击启动时，把已经在跑的窗口唤到前台，而不是抢占端口
            tray::show_console(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::service_status,
            commands::get_site_name,
            commands::save_site_name,
            commands::service_start,
            commands::service_stop,
            commands::access_addresses,
            commands::get_settings,
            commands::save_settings,
            commands::backup_now,
            commands::list_backups,
            commands::recent_logs,
            commands::app_info,
            commands::quit_app,
            commands::copy_text,
            commands::open_url,
            commands::open_path,
            commands::reveal_path,
        ])
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if !app.state::<AppContext>().is_quitting() {
                    // 关窗口不等于停服务：收进托盘，局域网用户继续能访问
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("初始化应用失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let context = app.state::<AppContext>();
                context.mark_quitting();
                context.service.stop();
                tracing::info!("流转平台已退出");
            }
        });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

    let default_data_dir = handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("./data"));
    let settings_file = handle
        .path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .unwrap_or_else(|_| default_data_dir.join("settings.json"));

    let settings = settings::Settings::load(&settings_file);
    let context = AppContext::new(
        Paths {
            settings_file,
            default_data_dir,
        },
        settings.clone(),
    );

    let config = context.server_config();
    if let Err(err) = config.ensure_dirs() {
        eprintln!("无法创建数据目录：{err}");
    }

    match circulation_server::config::init_tracing(&config) {
        Ok(guard) => {
            let _ = TRACING_GUARD.set(Mutex::new(guard));
        }
        Err(err) => eprintln!("日志初始化失败：{err}"),
    }

    tracing::info!(
        "流转平台 {} 启动，数据目录 {}",
        env!("CARGO_PKG_VERSION"),
        config.data_dir.display()
    );

    let should_start = settings.start_service_on_launch;
    app.manage(context);
    tray::build(&handle)?;

    if should_start {
        tauri::async_runtime::spawn(async move {
            let context = handle.state::<AppContext>();
            let config = context.server_config();
            match context.service.start(config).await {
                Ok(()) => tray::refresh(&handle, true),
                Err(err) => {
                    tracing::error!("自动启动服务失败：{err}");
                    context.service.record_error(err);
                    tray::refresh(&handle, false);
                }
            }
        });
    } else {
        tray::refresh(&handle, false);
    }

    Ok(())
}
