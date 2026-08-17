use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_plugin_store::StoreExt;

/// 是否关闭到托盘（而非退出应用）
fn should_close_to_tray(app: &AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("ui.close_to_tray").and_then(|v| v.as_bool()))
        .unwrap_or(false)
}

/// 是否最小化到托盘（隐藏而非停留 Dock/任务栏）
fn should_minimize_to_tray(app: &AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("ui.minimize_to_tray").and_then(|v| v.as_bool()))
        .unwrap_or(true)
}

/// 恢复主窗口（显示 + 取消最小化 + 聚焦）
pub fn restore_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            // macOS 的 App 可能整个被 hide，需先 app.show()
            let _ = app.show();
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

/// 创建系统托盘（图标 + 菜单 + 左键恢复窗口）
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("YeAPI - Local LLM API Gateway")
        .show_menu_on_left_click(false) // 左键不弹菜单（左键 = 恢复窗口）
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = restore_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => {
                let _ = restore_main_window(app);
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// 拦截窗口事件：关闭到托盘 / 最小化到托盘
pub fn setup_window_events(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if should_close_to_tray(&app_handle) {
                    api.prevent_close();
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            }
            // Tauri v2 无 Minimize 事件，用 Resized + is_minimized 兜底
            tauri::WindowEvent::Resized(_) => {
                if should_minimize_to_tray(&app_handle) {
                    if let Some(w) = app_handle.get_webview_window("main") {
                        if w.is_minimized().unwrap_or(false) {
                            let _ = w.hide();
                        }
                    }
                }
            }
            _ => {}
        });
    }
    Ok(())
}
