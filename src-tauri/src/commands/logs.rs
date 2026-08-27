use crate::AppState;
use crate::db::models::*;
use crate::db::repository::Repository;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GetLogsInput {
    pub api_key_name: Option<String>,
    pub channel_name: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub status_code: Option<i64>,
    pub is_stream: Option<i64>,
    pub is_retry: Option<i64>,
    pub risk_level: Option<String>,
    pub security_action: Option<String>,
    pub finding_rule: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[tauri::command]
pub async fn get_logs(
    input: GetLogsInput,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<RequestLog>, String> {
    let repo = Repository::new(state.db.pool.clone());
    let page = input.page.unwrap_or(1).max(1);
    let page_size = input.page_size.unwrap_or(20).min(200);
    let offset = (page - 1) * page_size;
    repo.search_logs(
        input.keyword.as_deref(),
        input.channel_name.as_deref(),
        input.model.as_deref(),
        input.mode.as_deref(),
        input.status_code,
        input.is_stream,
        input.is_retry,
        input.risk_level.as_deref(),
        input.security_action.as_deref(),
        input.finding_rule.as_deref(),
        input.start_date.as_deref(),
        input.end_date.as_deref(),
        page_size,
        offset,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_log(
    id: String,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<RequestLog, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_log(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_log(
    id: String,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.delete_log(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_log_stats(
    days: Option<i64>,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<LogStats>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_log_stats(days.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_mode_stats(
    days: Option<i64>,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<LogModeStats>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_mode_stats(days.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_log_findings(
    log_id: String,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<SecurityFindingRow>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_findings_by_log_id(&log_id)
        .await
        .map_err(|e| e.to_string())
}
