pub mod auth;
pub mod handlers;
pub mod quota;
pub mod router;

use crate::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

/// 网关启动/运行失败的快照，供前端提示与重试。
#[derive(Debug, Clone, Serialize)]
pub struct ServerErrorInfo {
    /// bind：端口占用或地址不可用；serve：服务运行期错误。
    pub kind: String,
    pub message: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub running: bool,
    pub host: String,
    /// 设置里配置的端口。
    pub port: u16,
    /// 实际绑定的端口；未运行时为 0。
    pub actual_port: u16,
    pub url: String,
    pub started_at: Option<String>,
    pub error: Option<ServerErrorInfo>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

async fn emit_status(app: &AppHandle, state: &Arc<AppState>) {
    let status = snapshot(app, state).await;
    app.emit("server-status-changed", status).ok();
}

/// 先完成绑定再返回，保证调用方与前端拿到的一定是可用端口。
/// 绑定失败会重试数次并把错误写入共享状态（不再静默）。
pub async fn start(app: AppHandle, state: Arc<AppState>) -> Result<ServerStatus, String> {
    let host = get_server_host(&app);
    let configured_port = get_server_port(&app);
    let addr = format!("{host}:{configured_port}");

    let mut listener = None;
    let mut last_error = String::new();
    for attempt in 0..6u32 {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(bound) => {
                listener = Some(bound);
                break;
            }
            Err(err) => {
                last_error = err.to_string();
                if attempt < 5 {
                    tokio::time::sleep(Duration::from_millis(100 + 50 * attempt as u64)).await;
                }
            }
        }
    }

    let Some(listener) = listener else {
        let info = ServerErrorInfo {
            kind: "bind".to_string(),
            message: format!("无法监听 {addr}: {last_error}"),
            at: now_iso(),
        };
        *state.server_error.write().await = Some(info.clone());
        state.server_running.store(false, Ordering::SeqCst);
        emit_status(&app, &state).await;
        return Err(info.message);
    };

    let actual_port = listener
        .local_addr()
        .map_err(|err| format!("读取监听地址失败: {err}"))?
        .port();
    *state.server_port.write().await = actual_port;
    *state.server_started_at.write().await = Some(now_iso());
    *state.server_error.write().await = None;
    state.server_running.store(true, Ordering::SeqCst);

    let url = format!("http://{host}:{actual_port}");
    app.emit(
        "server-started",
        serde_json::json!({ "port": actual_port, "url": url }),
    )
    .ok();
    tracing::info!("yeapi server listening on {url}");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    *state.server_shutdown.lock().await = Some(shutdown_tx);

    let router = router::create_router(app.clone(), state.clone());
    let serve_state = state.clone();
    let serve_app = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
        serve_state.server_running.store(false, Ordering::SeqCst);
        *serve_state.server_port.write().await = 0;
        if let Err(err) = result {
            let info = ServerErrorInfo {
                kind: "serve".to_string(),
                message: err.to_string(),
                at: now_iso(),
            };
            tracing::error!("yeapi server stopped with error: {}", info.message);
            *serve_state.server_error.write().await = Some(info);
        }
        emit_status(&serve_app, &serve_state).await;
    });
    *state.server_handle.write().unwrap() = Some(handle);

    emit_status(&app, &state).await;
    Ok(snapshot(&app, &state).await)
}

/// 停止网关：先发优雅关闭信号，等待任务退出；超时才强制 abort。
pub async fn stop(state: &Arc<AppState>) -> Result<(), String> {
    if let Some(tx) = state.server_shutdown.lock().await.take() {
        let _ = tx.send(());
    }

    for _ in 0..60 {
        if !state.server_running.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let still_running = state.server_running.load(Ordering::SeqCst);
    let handle = state.server_handle.write().unwrap().take();
    if still_running {
        if let Some(handle) = handle {
            handle.abort();
        }
        state.server_running.store(false, Ordering::SeqCst);
    }

    *state.server_port.write().await = 0;
    Ok(())
}

pub async fn restart(app: AppHandle, state: Arc<AppState>) -> Result<ServerStatus, String> {
    stop(&state).await?;
    start(app, state).await
}

pub async fn snapshot(app: &AppHandle, state: &Arc<AppState>) -> ServerStatus {
    let running = state.server_running.load(Ordering::SeqCst);
    let actual_port = *state.server_port.read().await;
    let host = get_server_host(app);
    let port = get_server_port(app);
    let url = if running && actual_port != 0 {
        format!("http://{host}:{actual_port}")
    } else {
        format!("http://{host}:{port}")
    };
    ServerStatus {
        running,
        host,
        port,
        actual_port,
        url,
        started_at: state.server_started_at.read().await.clone(),
        error: state.server_error.read().await.clone(),
    }
}

pub(crate) fn get_server_host(app: &AppHandle) -> String {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("server.host"))
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub(crate) fn get_server_port(app: &AppHandle) -> u16 {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("server.port"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u16)
        .unwrap_or(8777)
}
