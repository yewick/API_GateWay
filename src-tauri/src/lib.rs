mod adaptor;
mod commands;
mod core;
mod db;
mod protocol;
mod security;
mod server;
mod services;
mod tray;
mod utils;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::RwLock;
use tauri::Manager;

/// 全局应用状态
pub struct AppState {
    pub db: Arc<db::Database>,
    pub server_port: Arc<tokio::sync::RwLock<u16>>,
    pub server_running: Arc<AtomicBool>,
    pub server_handle: Arc<RwLock<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .invoke_handler(tauri::generate_handler![
            greet,
            // 渠道
            commands::channel::get_channels,
            commands::channel::create_channel,
            commands::channel::test_channel,
            commands::channel::toggle_channel,
            commands::channel::delete_channel,
            commands::channel::get_channel,
            commands::channel::update_channel,
            // 密钥
            commands::api_key::get_api_keys,
            commands::api_key::create_api_key,
            commands::api_key::update_api_key,
            commands::api_key::delete_api_key,
            // 日志
            commands::logs::get_logs,
            commands::logs::get_log,
            commands::logs::delete_log,
            commands::logs::get_log_stats,
            commands::logs::get_log_findings,
            // 仪表盘
            commands::dashboard::get_dashboard_stats,
            // 设置
            commands::settings::get_settings,
            commands::settings::save_settings,
            // 服务器
            commands::server::restart_server,
            // 安全规则
            commands::security::get_builtin_security_rules,
            commands::security::update_builtin_security_rule,
            commands::security::reset_builtin_security_rules,
            commands::security::get_custom_security_rules,
            commands::security::create_custom_security_rule,
            commands::security::toggle_custom_security_rule,
            commands::security::delete_custom_security_rule,
            // 测试台
            commands::test::send_test_request,
            // 服务状态
            commands::services::get_service_statuses,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                let db = db::Database::new(&handle).await;

                let state = Arc::new(AppState {
                    db: Arc::new(db),
                    server_port: Arc::new(tokio::sync::RwLock::new(0)),
                    server_running: Arc::new(AtomicBool::new(false)),
                    server_handle: Arc::new(RwLock::new(None)),
                });

                handle.manage(state.clone());

                // 系统托盘 + 窗口事件（关闭/最小化到托盘）
                tray::setup_tray(&handle)?;
                tray::setup_window_events(&handle)?;

                // 启动本地 HTTP 服务，并保存任务句柄以便重启
                let state_for_server = state.clone();
                let handle_for_server = handle.clone();
                let join = tauri::async_runtime::spawn(async move {
                    let _ = server::start_server(handle_for_server, state_for_server).await;
                });
                *state.server_handle.write().unwrap() = Some(join);

                Ok::<(), tauri::Error>(())
            })?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                let _ = tray::restore_main_window(app);
            }
        });
}
