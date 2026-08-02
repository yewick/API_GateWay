mod db;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::RwLock;
use tauri::Manager;

/// 全局应用状态
pub struct AppState {
    pub db: Arc<db::Database>,
    pub server_port: Arc<RwLock<u16>>,
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
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                let db = db::Database::new(&handle).await;

                let state = Arc::new(AppState {
                    db: Arc::new(db),
                    server_port: Arc::new(RwLock::new(0)),
                    server_running: Arc::new(AtomicBool::new(false)),
                    server_handle: Arc::new(RwLock::new(None)),
                });

                handle.manage(state.clone());
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
