pub mod auth;
pub mod router;
pub mod handlers;

use crate::AppState;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

pub async fn start_server(app: AppHandle, state: std::sync::Arc<AppState>) -> Result<(), anyhow::Error> {
    let host = get_server_host(&app);
    let port = get_server_port(&app);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let actual_port = listener.local_addr()?.port();

    // 更新共享状态（前端通过命令查询）
    *state.server_port.write().await = actual_port;
    state.server_running.store(true, std::sync::atomic::Ordering::SeqCst);

    let router = router::create_router(app.clone(), state.clone());

    // 通知前端服务器已启动（前端监听此事件更新 UI）
    app.emit("server-started", serde_json::json!({
        "port": actual_port,
        "url": format!("http://{}:{}", host, actual_port)
    })).ok();

    tracing::info!("yeapi server listening on http://{}:{}", host, actual_port);

    // 启动 Axum 服务（阻塞直到服务器停止）
    axum::serve(listener, router).await?;

    state.server_running.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

fn get_server_host(app: &AppHandle) -> String {
    app.store("settings.json").ok()
        .and_then(|store| store.get("server.host"))
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn get_server_port(app: &AppHandle) -> u16 {
    app.store("settings.json").ok()
        .and_then(|store| store.get("server.port"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u16)
        .unwrap_or(8777)
}

