use tauri::State;
use std::sync::Arc;
use crate::AppState;
use crate::db::models::*;
use crate::db::repository::Repository;

#[tauri::command]
pub async fn get_api_keys(state: State<'_, Arc<AppState>>) -> Result<Vec<ApiKey>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_all_api_keys().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_api_key(
    state: State<'_, Arc<AppState>>,
    input: CreateApiKeyInput,
) -> Result<ApiKey, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.create_api_key(&input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_api_key(
    state: State<'_, Arc<AppState>>,
    input: UpdateApiKeyInput,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.update_api_key(&input.id, input.status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_api_key(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.delete_api_key(&id).await.map_err(|e| e.to_string())
}
