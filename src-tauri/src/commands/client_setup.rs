//! 客户端接入相关的 Tauri 命令。
//!
//! 明文 API Key 只在 Rust 侧取用并写入客户端配置文件，不经过 IPC 回传前端。

use crate::client_setup::{
    self, codex, paths, state as setup_state, ApiKeySelection, ApplyClientInput, ApplyClientResult,
    ClientKind, ClientSetupStatus, ClientTestResult, DisconnectClientResult, ModelOption,
    PreviewResult,
};
use crate::db::models::CreateApiKeyInput;
use crate::db::repository::Repository;
use crate::AppState;
use std::sync::Arc;

fn require_codex(client: &str) -> Result<(), String> {
    match ClientKind::parse(client)? {
        ClientKind::Codex => Ok(()),
    }
}

/// 检查模型能否命中至少一个启用渠道；不能时返回可直接展示给用户的提示。
async fn model_availability(repo: &Repository, model: &str) -> Result<Option<String>, String> {
    let channels = repo
        .get_enabled_channels()
        .await
        .map_err(|err| err.to_string())?;
    if !crate::core::dispatcher::Dispatcher::select_channels(&channels, model).is_empty() {
        return Ok(None);
    }

    let mut names: Vec<String> = crate::server::handlers::collect_models(&channels)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    names.sort();
    names.dedup();
    let hint = if names.is_empty() {
        "当前没有启用渠道，请先在「渠道」页添加并启用渠道".to_string()
    } else {
        format!("当前可用模型：{}", names.join("、"))
    };
    Ok(Some(format!(
        "模型 `{model}` 没有匹配的启用渠道。{hint}。请到「渠道」页启用包含该模型的渠道，或换一个模型。"
    )))
}

#[tauri::command]
pub async fn client_setup_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<ClientSetupStatus, String> {
    require_codex(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = paths::resolve_codex_paths()?;
    let store = setup_state::load_store(&app_data)?;
    let record = store.clients.get(ClientKind::Codex.as_str());
    let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
    let mut status = codex::status(&paths, record, &gateway);

    if let Some(record) = record {
        let repo = Repository::new(state.db.pool.clone());
        status.applied_api_key_valid = repo
            .get_api_key_by_id(&record.api_key_id)
            .await
            .ok()
            .is_some_and(|key| {
                !crate::server::auth::key_is_expired(key.expires_at.as_deref().unwrap_or(""))
            });
    }

    Ok(status)
}

#[tauri::command]
pub async fn client_setup_list_models(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<ModelOption>, String> {
    let repo = Repository::new(state.db.pool.clone());
    let channels = repo
        .get_enabled_channels()
        .await
        .map_err(|err| err.to_string())?;
    Ok(crate::server::handlers::collect_models(&channels)
        .into_iter()
        .map(|(id, owned_by)| ModelOption { id, owned_by })
        .collect())
}

#[tauri::command]
pub async fn client_setup_preview(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    input: ApplyClientInput,
) -> Result<PreviewResult, String> {
    require_codex(&input.client)?;

    let model = input.model.trim();
    if model.is_empty() {
        return Err("请选择或输入默认模型".to_string());
    }

    let paths = paths::resolve_codex_paths()?;
    let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
    let requires_openai_auth = codex::auth_logged_in(&paths.auth);

    let repo = Repository::new(state.db.pool.clone());
    let warning = model_availability(&repo, model).await?;

    // 只生成 YeAPI 会写入/覆盖的键；实际写入时才会合并进原文件。
    let snippet = codex::merge_config(
        "",
        &codex::ProviderSpec {
            base_url: &gateway,
            model,
            api_key: "<已隐藏，写入时使用所选密钥>",
            requires_openai_auth,
        },
    )?;

    Ok(PreviewResult {
        config_path: paths.config.to_string_lossy().to_string(),
        snippet,
        warning,
    })
}

#[tauri::command]
pub async fn client_setup_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    input: ApplyClientInput,
) -> Result<ApplyClientResult, String> {
    require_codex(&input.client)?;

    let model = input.model.trim().to_string();
    if model.is_empty() {
        return Err("请选择或输入默认模型".to_string());
    }

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = paths::resolve_codex_paths()?;
    std::fs::create_dir_all(&paths.home)
        .map_err(|err| format!("创建 {} 失败: {err}", paths.home.display()))?;

    let _guard = client_setup::SETUP_LOCK.lock().await;

    let mut store = setup_state::load_store(&app_data)?;
    let existing_record = store.clients.get(ClientKind::Codex.as_str()).cloned();
    let current_home = paths.home.to_string_lossy().to_string();
    if let Some(record) = &existing_record {
        if record.codex_home != current_home {
            return Err(format!(
                "检测到接入记录指向其他 CODEX_HOME：{}。请先在原目录断开接入",
                record.codex_home
            ));
        }
    }

    let existing_text = setup_state::read_optional(&paths.config)?.unwrap_or_default();
    let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
    let requires_openai_auth = codex::auth_logged_in(&paths.auth);

    let repo = Repository::new(state.db.pool.clone());
    if let Some(warning) = model_availability(&repo, &model).await? {
        return Err(warning);
    }
    let created_new_key = matches!(&input.key, ApiKeySelection::New { .. });
    let (key_id, api_key) = match &input.key {
        ApiKeySelection::Existing { id } => {
            let key = repo
                .get_api_key_by_id(id)
                .await
                .map_err(|_| "所选密钥不存在或已禁用".to_string())?;
            if key
                .expires_at
                .as_deref()
                .is_some_and(crate::server::auth::key_is_expired)
            {
                return Err("所选密钥已过期".to_string());
            }
            (key.id, key.key)
        }
        ApiKeySelection::New {
            name,
            allowed_models,
            quota_limit,
            expires_at,
        } => {
            let name = if name.trim().is_empty() {
                "Codex".to_string()
            } else {
                name.trim().to_string()
            };
            let created = repo
                .create_api_key(&CreateApiKeyInput {
                    name,
                    allowed_models: allowed_models.clone(),
                    allowed_channels: Vec::new(),
                    quota_limit: Some(quota_limit.unwrap_or(-1)),
                    expires_at: expires_at.clone(),
                })
                .await
                .map_err(|err| format!("创建密钥失败: {err}"))?;
            (created.id, created.key)
        }
    };
    let key_fingerprint = setup_state::fingerprint(&api_key);

    // 生成 Codex 模型目录，让模型菜单只出现当前启用渠道真正支持的模型。
    let channels = repo
        .get_enabled_channels()
        .await
        .map_err(|err| err.to_string())?;
    let mut catalog_models: Vec<String> = crate::server::handlers::collect_models(&channels)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    catalog_models.sort();
    catalog_models.dedup();
    if !catalog_models
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&model))
    {
        catalog_models.push(model.clone());
    }
    let catalog_path = paths.home.join(codex::CATALOG_FILENAME);
    if let Err(err) = setup_state::atomic_write(
        &catalog_path,
        codex::build_model_catalog(&catalog_models).as_bytes(),
        0o644,
    ) {
        if created_new_key {
            let _ = repo.delete_api_key(&key_id).await;
        }
        return Err(format!("生成模型目录失败: {err}"));
    }

    let mut backup_path = existing_record
        .as_ref()
        .and_then(|record| record.backup_path.clone());
    if existing_record.is_none() && paths.config.exists() {
        match setup_state::backup_config(&paths.config, &app_data) {
            Ok(path) => backup_path = Some(path.to_string_lossy().to_string()),
            Err(err) => {
                if created_new_key {
                    let _ = repo.delete_api_key(&key_id).await;
                }
                return Err(err);
            }
        }
    }

    let merged = match codex::merge_config(
        &existing_text,
        &codex::ProviderSpec {
            base_url: &gateway,
            model: &model,
            api_key: &api_key,
            requires_openai_auth,
        },
    ) {
        Ok(text) => text,
        Err(err) => {
            if created_new_key {
                let _ = repo.delete_api_key(&key_id).await;
            }
            return Err(err);
        }
    };

    if let Err(err) = setup_state::atomic_write(&paths.config, merged.as_bytes(), 0o600) {
        if created_new_key {
            let _ = repo.delete_api_key(&key_id).await;
        }
        return Err(err);
    }

    let previous = existing_record
        .as_ref()
        .map(|record| record.previous.clone())
        .unwrap_or_else(|| codex::capture(&existing_text));

    let record = setup_state::ClientRecord {
        applied_at: chrono::Utc::now().to_rfc3339(),
        codex_home: current_home,
        config_path: paths.config.to_string_lossy().to_string(),
        provider_id: codex::PROVIDER_ID.to_string(),
        base_url: gateway.clone(),
        model: model.clone(),
        api_key_id: key_id,
        key_fingerprint,
        previous,
        backup_path: backup_path.clone(),
    };
    store
        .clients
        .insert(ClientKind::Codex.as_str().to_string(), record);

    if let Err(err) = setup_state::save_store(&app_data, &store) {
        return Err(format!("config.toml 已写入，但保存接入记录失败: {err}"));
    }

    let record = store.clients.get(ClientKind::Codex.as_str());
    let mut status = codex::status(&paths, record, &gateway);
    status.applied_api_key_valid = true;

    Ok(ApplyClientResult {
        status,
        backup_path,
        restart_required: true,
    })
}

#[tauri::command]
pub async fn client_setup_disconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<DisconnectClientResult, String> {
    require_codex(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = paths::resolve_codex_paths()?;

    let _guard = client_setup::SETUP_LOCK.lock().await;

    let mut store = setup_state::load_store(&app_data)?;
    let record = store
        .clients
        .get(ClientKind::Codex.as_str())
        .cloned()
        .ok_or_else(|| "未找到 Codex 接入记录".to_string())?;

    let existing = setup_state::read_optional(&paths.config)?.unwrap_or_default();
    if existing.trim().is_empty() {
        store.clients.remove(ClientKind::Codex.as_str());
        setup_state::save_store(&app_data, &store)?;
        let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
        return Ok(DisconnectClientResult {
            status: codex::status(&paths, None, &gateway),
            backup_path: record.backup_path.clone(),
            warnings: vec!["config.toml 不存在，仅清除了接入记录".to_string()],
        });
    }

    let (restored, output) = codex::disconnect_config(&existing, &record)?;
    if output.remove_config_file {
        std::fs::remove_file(&paths.config)
            .map_err(|err| format!("删除 {} 失败: {err}", paths.config.display()))?;
    } else {
        setup_state::atomic_write(&paths.config, restored.as_bytes(), 0o600)?;
    }

    store.clients.remove(ClientKind::Codex.as_str());
    setup_state::save_store(&app_data, &store)?;

    let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
    Ok(DisconnectClientResult {
        status: codex::status(&paths, None, &gateway),
        backup_path: record.backup_path.clone(),
        warnings: output.warnings,
    })
}

#[tauri::command]
pub async fn client_setup_test(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<ClientTestResult, String> {
    require_codex(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let store = setup_state::load_store(&app_data)?;
    let record = store
        .clients
        .get(ClientKind::Codex.as_str())
        .cloned()
        .ok_or_else(|| "请先完成 Codex 接入".to_string())?;

    let repo = Repository::new(state.db.pool.clone());
    if let Some(warning) = model_availability(&repo, &record.model).await? {
        return Err(warning);
    }
    let key = repo
        .get_api_key_by_id(&record.api_key_id)
        .await
        .map_err(|_| "接入使用的密钥已被删除或禁用，请重新接入".to_string())?;
    if key
        .expires_at
        .as_deref()
        .is_some_and(crate::server::auth::key_is_expired)
    {
        return Err("接入使用的密钥已过期，请重新接入".to_string());
    }

    let url = format!("{}/responses", record.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": record.model,
        "input": "回复 OK 两个字母即可",
        "max_output_tokens": 32,
        "stream": false,
    });

    let started = std::time::Instant::now();
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;
    let response = http
        .post(&url)
        .bearer_auth(&key.key)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("请求网关失败: {err}"))?;

    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let latency_ms = started.elapsed().as_millis() as u64;
    let message = if status == 503 && text.contains("No channel available") {
        format!(
            "模型 `{}` 当前没有可用渠道。请到「渠道」页启用包含该模型的渠道，或重新接入时选择其他模型。",
            record.model
        )
    } else if status < 400 {
        extract_response_text(&text).unwrap_or_else(|| "网关已响应".to_string())
    } else {
        format!("网关返回 {status}: {}", truncate(&text, 300))
    };

    Ok(ClientTestResult {
        ok: status < 400,
        status,
        latency_ms,
        message,
    })
}

#[tauri::command]
pub fn client_setup_open_dir(app: tauri::AppHandle, client: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    require_codex(&client)?;

    let paths = paths::resolve_codex_paths()?;
    std::fs::create_dir_all(&paths.home)
        .map_err(|err| format!("创建 {} 失败: {err}", paths.home.display()))?;

    if paths.config.exists() {
        app.opener()
            .reveal_item_in_dir(&paths.config)
            .map_err(|err| format!("打开配置目录失败: {err}"))
    } else {
        app.opener()
            .open_path(paths.home.to_string_lossy().to_string(), None::<&str>)
            .map_err(|err| format!("打开配置目录失败: {err}"))
    }
}

fn extract_response_text(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if let Some(output) = value.get("output").and_then(|item| item.as_array()) {
        for item in output {
            if let Some(content) = item.get("content").and_then(|item| item.as_array()) {
                for part in content {
                    if let Some(text) = part.get("text").and_then(|part| part.as_str()) {
                        if !text.trim().is_empty() {
                            return Some(text.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    if let Some(text) = value.get("output_text").and_then(|item| item.as_str()) {
        if !text.trim().is_empty() {
            return Some(text.trim().to_string());
        }
    }
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .map(str::to_string)
}

fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let mut short: String = trimmed.chars().take(max_chars).collect();
        short.push('…');
        short
    }
}
