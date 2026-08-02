use tauri::State;
use std::sync::Arc;
use crate::AppState;
use crate::db::models::*;
use crate::db::repository::Repository;
use crate::adaptor::{get_adaptor, ChannelConfig, TestResult};

#[tauri::command]
pub async fn get_channels(state: State<'_, Arc<AppState>>) -> Result<Vec<Channel>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_all_channels().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_channel(
    state: State<'_, Arc<AppState>>,
    input: CreateChannelInput,
) -> Result<Channel, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.create_channel(&input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_channel(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<TestResult, String> {
    let repo = Repository::new(state.db.pool.clone());
    let channel = repo.get_channel(&id).await.map_err(|e| e.to_string())?;

    // Channel → ChannelConfig 转换
    let models: Vec<String> = serde_json::from_str(&channel.models).unwrap_or_default();
    let config = ChannelConfig {
        base_url: channel.base_url.clone(),
        api_key: channel.api_key.clone(),
        models,
        model_mapping: serde_json::from_str(&channel.model_mapping).unwrap_or_default(),
        extra: serde_json::from_str(&channel.config).unwrap_or_default(),
    };

    let adaptor = get_adaptor(&channel.channel_type);
    let result = adaptor.test(&config).await.map_err(|e| e.to_string())?;

    // 记录测试结果到数据库（前端列表展示最近测试状态）
    let _ = repo.update_channel_test_result(&id, result.success).await;

    Ok(result)
}