use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::AppState;

/// 重启本地 HTTP 服务：停掉旧任务，按最新配置重新绑定端口
#[tauri::command]
pub async fn restart_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // 停掉旧服务器任务
    if let Some(handle) = state.server_handle.write().unwrap().take() {
        handle.abort();
    }
    state.server_running.store(false, Ordering::SeqCst);

    // 启动新服务器
    let handle = app.clone();
    let state_clone = state.inner().clone();
    let join = tauri::async_runtime::spawn(async move {
        let _ = crate::server::start_server(handle, state_clone).await;
    });
    *state.server_handle.write().unwrap() = Some(join);

    Ok(())
}
