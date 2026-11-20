use tauri::State;
use std::sync::Arc;
use crate::AppState;
use crate::client_setup::reconcile::{self, RepairMode};
use crate::db::models::*;
use crate::db::repository::Repository;
use crate::adaptor::{get_adaptor, ChannelConfig, TestResult};

/// 渠道变化后刷新 Codex 模型目录（自检只会重建「仍由 YeAPI 持有」的配置）。
fn spawn_catalog_refresh(app: tauri::AppHandle, state: &State<'_, Arc<AppState>>) {
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let report = reconcile::reconcile_all(&app, &state, RepairMode::Auto).await;
        *state.reconcile_report.write().await = Some(report);
    });
}

#[tauri::command]
pub async fn get_channels(state: State<'_, Arc<AppState>>) -> Result<Vec<Channel>, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_all_channels().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_channel(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    input: CreateChannelInput,
) -> Result<Channel, String> {
    let repo = Repository::new(state.db.pool.clone());
    let channel = repo.create_channel(&input).await.map_err(|e| e.to_string())?;
    spawn_catalog_refresh(app, &state);
    Ok(channel)
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

#[tauri::command]
pub async fn toggle_channel(
    app: tauri::AppHandle,
    id: String,
    status: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.update_channel(&UpdateChannelInput {
        id,
        name: None,
        channel_type: None,
        base_url: None,
        api_key: None,
        models: None,
        status: Some(status),
        priority: None,
        weight: None,
        config: None,
        model_mapping: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    spawn_catalog_refresh(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn delete_channel(
    app: tauri::AppHandle,
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.delete_channel(&id).await.map_err(|e| e.to_string())?;
    spawn_catalog_refresh(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn get_channel(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Channel, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_channel(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_channel(
    app: tauri::AppHandle,
    input: UpdateChannelInput,
    state: State<'_, Arc<AppState>>,
) -> Result<Channel, String> {
    let repo = Repository::new(state.db.pool.clone());
    let channel = repo.update_channel(&input).await.map_err(|e| e.to_string())?;
    spawn_catalog_refresh(app, &state);
    Ok(channel)
}

#[tauri::command]
pub async fn reorder_channels(
    ids: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.reorder_channels(&ids).await.map_err(|e| e.to_string())
}