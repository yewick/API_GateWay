mod adaptor;
mod commands;
mod core;
mod db;
mod security;
mod server;
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
    pub server_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
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
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::channel::get_channels,
            commands::channel::create_channel,
            commands::channel::test_channel,
            commands::api_key::get_api_keys,
            commands::api_key::create_api_key,
            commands::api_key::update_api_key,
            commands::api_key::delete_api_key,
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

                tauri::async_runtime::spawn(async move {
                    let _ = server::start_server(handle, state).await;
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
