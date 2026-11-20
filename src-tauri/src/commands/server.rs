use std::sync::Arc;

use crate::AppState;
use crate::client_setup::reconcile::{self, RepairMode};
use crate::server::ServerStatus;

/// 网关当前状态：运行中 / 未运行 / 绑定失败。
#[tauri::command]
pub async fn server_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ServerStatus, String> {
    Ok(crate::server::snapshot(&app, state.inner()).await)
}

/// 重启本地 HTTP 服务：优雅停止 → 重新绑定 → 返回真实状态。
/// 端口变化后自动触发一次客户端接入自检，改写客户端指向的网关地址。
#[tauri::command]
pub async fn restart_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ServerStatus, String> {
    let status = crate::server::restart(app.clone(), state.inner().clone()).await?;

    let reconcile_state = state.inner().clone();
    let reconcile_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let report = reconcile::reconcile_all(&reconcile_app, &reconcile_state, RepairMode::Auto).await;
        *reconcile_state.reconcile_report.write().await = Some(report);
    });

    Ok(status)
}
