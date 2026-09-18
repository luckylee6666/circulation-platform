use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::net;
use crate::AppContext;

/// 显示并聚焦主窗口。窗口被关闭时只是隐藏，服务仍在后台运行。
pub fn show_console<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("console") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "服务未运行", false, None::<&str>)?;
    let open_console = MenuItem::with_id(app, "console", "打开控制台", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "启动服务", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "copy", "复制访问地址", true, None::<&str>)?;
    let browser = MenuItem::with_id(app, "browser", "在浏览器中打开", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[&status, &separator, &open_console, &toggle, &copy, &browser, &separator, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("流转平台")
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_console(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// 托盘菜单里的状态文案与「启动/停止」项需要随服务状态变化，直接重建菜单最简单可靠。
pub fn refresh<R: Runtime>(app: &AppHandle<R>, running: bool) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };

    let (
        Ok(status),
        Ok(open_console),
        Ok(toggle),
        Ok(copy),
        Ok(browser),
        Ok(quit),
        Ok(separator),
        Ok(separator_end),
    ) = (
        MenuItem::with_id(app, "status", status_text(running), false, None::<&str>),
        MenuItem::with_id(app, "console", "打开控制台", true, None::<&str>),
        MenuItem::with_id(app, "toggle", toggle_text(running), true, None::<&str>),
        MenuItem::with_id(app, "copy", "复制访问地址", true, None::<&str>),
        MenuItem::with_id(app, "browser", "在浏览器中打开", true, None::<&str>),
        MenuItem::with_id(app, "quit", "退出", true, None::<&str>),
        PredefinedMenuItem::separator(app),
        PredefinedMenuItem::separator(app),
    )
    else {
        return;
    };

    let Ok(menu) = Menu::with_items(
        app,
        &[
            &status,
            &separator,
            &open_console,
            &toggle,
            &copy,
            &browser,
            &separator_end,
            &quit,
        ],
    ) else {
        return;
    };

    let _ = tray.set_menu(Some(menu));
}

fn status_text(running: bool) -> &'static str {
    if running {
        "服务运行中"
    } else {
        "服务未运行"
    }
}

fn toggle_text(running: bool) -> &'static str {
    if running {
        "停止服务"
    } else {
        "启动服务"
    }
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        "console" => show_console(app),
        "toggle" => toggle_service(app.clone()),
        "copy" => copy_primary_address(app),
        "browser" => open_in_browser(app),
        "quit" => {
            let context = app.state::<AppContext>();
            context.mark_quitting();
            context.service.stop();
            app.exit(0);
        }
        _ => {}
    }
}

fn toggle_service<R: Runtime>(app: AppHandle<R>) {
    let context = app.state::<AppContext>();

    if context.service.is_running() {
        context.service.stop();
        refresh(&app, false);
        return;
    }

    let config = context.server_config();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let context = handle.state::<AppContext>();
        let running = context.service.start(config).await.is_ok();
        if !running {
            context.service.record_error("从托盘启动服务失败，请打开控制台查看原因".to_string());
        }
        refresh(&handle, running);
    });
}

fn primary_address<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let context = app.state::<AppContext>();
    let port = if context.service.is_running() {
        context.service.port()
    } else {
        context.settings().port
    };
    net::lan_addresses(port)
        .into_iter()
        .find(|address| address.is_primary)
        .or_else(|| net::lan_addresses(port).into_iter().next())
        .map(|address| address.url)
}

fn copy_primary_address<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let Some(address) = primary_address(app) else {
        tracing::warn!("没有可用的局域网地址可复制");
        return;
    };

    if let Err(err) = app.clipboard().write_text(address) {
        tracing::warn!("写入剪贴板失败：{err}");
    }
}

fn open_in_browser<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_opener::OpenerExt;

    let Some(address) = primary_address(app) else {
        return;
    };

    if let Err(err) = app.opener().open_url(address, None::<&str>) {
        tracing::warn!("打开浏览器失败：{err}");
    }
}
