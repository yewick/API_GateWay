use crate::AppState;
use crate::db::models::*;
use crate::db::repository::Repository;
use serde::{Deserialize, Serialize};

/// 返回给前端的 DTO：JSON 字符串字段解析为数组
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyDto {
    pub id: String,
    pub name: String,
    pub key: String,
    pub status: i64,
    pub allowed_models: Vec<String>,      // 数据库存 JSON 字符串 → 前端用数组
    pub allowed_channels: Vec<String>,
    pub quota_limit: i64,
    pub quota_used: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ApiKey> for ApiKeyDto {
    fn from(k: ApiKey) -> Self {
        ApiKeyDto {
            id: k.id,
            name: k.name,
            key: k.key,
            status: k.status,
            // 存储层 JSON 字符串 → 领域层 Vec<String>
            allowed_models: serde_json::from_str(&k.allowed_models).unwrap_or_default(),
            allowed_channels: serde_json::from_str(&k.allowed_channels).unwrap_or_default(),
            quota_limit: k.quota_limit,
            quota_used: k.quota_used,
            expires_at: k.expires_at,
            created_at: k.created_at,
            updated_at: k.updated_at,
        }
    }
}

#[tauri::command]
pub async fn get_api_keys(
    state: tauri::State<'_, std::sync::Arc<AppState>>
) -> Result<Vec<ApiKeyDto>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_all_api_keys().await
        .map_err(|e| e.to_string())
        .map(|ks| ks.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn create_api_key(
    input: CreateApiKeyInput,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<ApiKeyDto, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.create_api_key(&input).await
        .map_err(|e| e.to_string())
        .map(Into::into)
}

#[derive(Debug, Deserialize)]
pub struct UpdateApiKeyInput {
    pub id: String,
    pub status: Option<i64>,
}

#[tauri::command]
pub async fn update_api_key(
    input: UpdateApiKeyInput,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    if let Some(status) = input.status {
        repo.update_api_key(&input.id, status).await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_api_key(
    id: String,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.delete_api_key(&id).await.map_err(|e| e.to_string())
}
