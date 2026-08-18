use tauri::State;
use std::sync::Arc;
use crate::AppState;
use crate::services::ServiceRegistry;

/// 获取所有服务状态（知识库、MCP 等），供前端统一展示
#[tauri::command]
pub async fn get_service_statuses(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<serde_json::Value>, String> {
    let registry = ServiceRegistry::new();
    let statuses = registry.list_status(state.inner()).await;
    Ok(statuses
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap_or_default())
        .collect())
}
