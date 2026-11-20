//! 客户端接入相关的 Tauri 命令。
//!
//! 明文 API Key 只在 Rust 侧取用并写入客户端配置文件，不经过 IPC 回传前端。

use crate::client_setup::{
    self, claude_code, claude_desktop, codex, paths, reconcile, state as setup_state,
    ApiKeySelection, ApplyClientInput, ApplyClientResult, ClaudeModelAliases, ClientKind,
    ClientSetupStatus, ClientTestResult, DesktopPreview, DisconnectClientResult, ModelOption,
    PreviewResult,
};
use crate::db::models::CreateApiKeyInput;
use crate::db::repository::Repository;
use crate::AppState;
use std::sync::Arc;
use tauri_plugin_store::StoreExt;

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

async fn aliases_availability(
    repo: &Repository,
    aliases: &ClaudeModelAliases,
) -> Result<Option<String>, String> {
    let mut warnings = Vec::new();
    for model in aliases.distinct_models() {
        if let Some(warning) = model_availability(repo, &model).await? {
            warnings.push(warning);
        }
    }
    Ok((!warnings.is_empty()).then(|| warnings.join("\n")))
}

async fn gateway_for(kind: ClientKind, app: &tauri::AppHandle, state: &crate::AppState) -> String {
    match kind {
        ClientKind::Codex => client_setup::gateway_base_url(app, state).await,
        ClientKind::Claude => client_setup::gateway_origin(app, state).await,
    }
}

/// 桌面端模型路由别名（`claude-desktop` 路由名 → YeAPI 真实模型）。
const DESKTOP_ALIAS_KEY: &str = "claude.desktop_aliases";

fn read_desktop_aliases(app: &tauri::AppHandle) -> Option<serde_json::Value> {
    let store = app.store("settings.json").ok()?;
    store.get(DESKTOP_ALIAS_KEY)
}

fn write_desktop_aliases(app: &tauri::AppHandle, value: serde_json::Value) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|err| format!("打开设置存储失败: {err}"))?;
    store.set(DESKTOP_ALIAS_KEY, value);
    store
        .save()
        .map_err(|err| format!("保存模型路由别名失败: {err}"))
}

fn restore_desktop_aliases(
    app: &tauri::AppHandle,
    previous: &Option<serde_json::Value>,
) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|err| format!("打开设置存储失败: {err}"))?;
    match previous {
        Some(value) => store.set(DESKTOP_ALIAS_KEY, value.clone()),
        None => {
            store.delete(DESKTOP_ALIAS_KEY);
        }
    }
    store
        .save()
        .map_err(|err| format!("恢复模型路由别名失败: {err}"))
}

struct ResolvedKey {
    id: String,
    key: String,
    created_new: bool,
    name: String,
    allowed_models: Vec<String>,
    quota_limit: Option<i64>,
}

async fn resolve_api_key(
    repo: &Repository,
    key: &ApiKeySelection,
    default_name: &str,
) -> Result<ResolvedKey, String> {
    match key {
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
            Ok(ResolvedKey {
                id: key.id,
                key: key.key,
                created_new: false,
                name: key.name,
                allowed_models: serde_json::from_str(&key.allowed_models).unwrap_or_default(),
                quota_limit: Some(key.quota_limit),
            })
        }
        ApiKeySelection::New {
            name,
            allowed_models,
            quota_limit,
            expires_at,
        } => {
            let name = if name.trim().is_empty() {
                default_name.to_string()
            } else {
                name.trim().to_string()
            };
            let created = repo
                .create_api_key(&CreateApiKeyInput {
                    name: name.clone(),
                    allowed_models: allowed_models.clone(),
                    allowed_channels: Vec::new(),
                    quota_limit: Some(quota_limit.unwrap_or(-1)),
                    expires_at: expires_at.clone(),
                })
                .await
                .map_err(|err| format!("创建密钥失败: {err}"))?;
            Ok(ResolvedKey {
                id: created.id,
                key: created.key,
                created_new: true,
                name,
                allowed_models: allowed_models.clone(),
                quota_limit: Some(quota_limit.unwrap_or(-1)),
            })
        }
    }
}

async fn rollback_key(repo: &Repository, key_id: &str, created_new_key: bool) {
    if created_new_key {
        let _ = repo.delete_api_key(key_id).await;
    }
}

async fn applied_key_valid(repo: &Repository, record: &setup_state::ClientRecord) -> bool {
    repo.get_api_key_by_id(&record.api_key_id)
        .await
        .ok()
        .is_some_and(|key| {
            !crate::server::auth::key_is_expired(key.expires_at.as_deref().unwrap_or(""))
        })
}

fn status_for(
    kind: ClientKind,
    paths: &client_setup::ClientPaths,
    record: Option<&setup_state::ClientRecord>,
    gateway: &str,
) -> ClientSetupStatus {
    match kind {
        ClientKind::Codex => codex::status(paths, record, gateway),
        ClientKind::Claude => {
            let desktop_dir = paths::resolve_claude_desktop_dir();
            let desktop = claude_desktop::status(desktop_dir.as_deref(), record, gateway);
            claude_code::status(paths, record, gateway, desktop)
        }
    }
}

#[tauri::command]
pub async fn client_setup_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<ClientSetupStatus, String> {
    let kind = ClientKind::parse(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = client_setup::resolve_paths(kind)?;
    let store = setup_state::load_store(&app_data)?;
    let record = store.clients.get(kind.as_str());
    let gateway = gateway_for(kind, &app, state.inner()).await;
    let mut status = status_for(kind, &paths, record, &gateway);

    if let Some(record) = record {
        let repo = Repository::new(state.db.pool.clone());
        status.applied_api_key_valid = applied_key_valid(&repo, record).await;
    }

    let inspection = reconcile::inspect(&app, state.inner(), kind, record).await;
    status.owned = inspection.owned;
    status.key_state = inspection.key_state;
    status.external_conflict = inspection.external_conflict;
    status.catalog_stale = inspection.catalog_stale;
    status.issues = inspection.issues;

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
    let kind = ClientKind::parse(&input.client)?;

    let model = input.model.trim();
    if model.is_empty() {
        return Err("请选择或输入默认模型".to_string());
    }

    let paths = client_setup::resolve_paths(kind)?;
    let gateway = gateway_for(kind, &app, state.inner()).await;
    let repo = Repository::new(state.db.pool.clone());

    match kind {
        ClientKind::Codex => {
            let requires_openai_auth = paths
                .auth
                .as_deref()
                .map(codex::auth_logged_in)
                .unwrap_or(false);
            let warning = model_availability(&repo, model).await?;
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
                desktop: None,
            })
        }
        ClientKind::Claude => {
            let options = input.claude.clone().unwrap_or_default();
            let aliases = ClaudeModelAliases::resolve(model, Some(&options));
            let warning = aliases_availability(&repo, &aliases).await?;
            let snippet = claude_code::merge_settings(
                "",
                &claude_code::SettingsSpec {
                    base_url: &gateway,
                    api_key: client_setup::MASKED_KEY,
                    models: &aliases,
                },
            )?;
            let desktop_dir = paths::resolve_claude_desktop_dir();
            let desktop = claude_desktop::preview(
                desktop_dir.as_deref(),
                &claude_desktop::DesktopSpec {
                    base_url: &gateway,
                    api_key: client_setup::MASKED_KEY,
                    models: &aliases,
                },
            );

            Ok(PreviewResult {
                config_path: paths.config.to_string_lossy().to_string(),
                snippet,
                warning,
                desktop: Some(DesktopPreview {
                    config_path: desktop.config_path,
                    snippet: desktop.snippet,
                    warning: desktop.warning,
                }),
            })
        }
    }
}

#[tauri::command]
pub async fn client_setup_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    input: ApplyClientInput,
) -> Result<ApplyClientResult, String> {
    let kind = ClientKind::parse(&input.client)?;

    let model = input.model.trim().to_string();
    if model.is_empty() {
        return Err("请选择或输入默认模型".to_string());
    }
    let options = input.claude.clone().unwrap_or_default();
    let aliases = ClaudeModelAliases::resolve(&model, Some(&options));

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = client_setup::resolve_paths(kind)?;
    std::fs::create_dir_all(&paths.home)
        .map_err(|err| format!("创建 {} 失败: {err}", paths.home.display()))?;

    let _guard = client_setup::SETUP_LOCK.lock().await;

    let mut store = setup_state::load_store(&app_data)?;
    let existing_record = store.clients.get(kind.as_str()).cloned();
    let current_home = paths.home.to_string_lossy().to_string();
    if let Some(record) = &existing_record {
        if record.config_home != current_home {
            return Err(format!(
                "检测到接入记录指向其他配置目录：{}。请先在原目录断开接入",
                record.config_home
            ));
        }
    }

    let existing_text = setup_state::read_optional(&paths.config)?.unwrap_or_default();
    let gateway = gateway_for(kind, &app, state.inner()).await;
    let repo = Repository::new(state.db.pool.clone());

    let warning = match kind {
        ClientKind::Codex => model_availability(&repo, &model).await?,
        ClientKind::Claude => {
            if !options.apply_cli && !options.apply_desktop {
                return Err("请至少选择一个接入范围（CLI 或桌面端）".to_string());
            }
            aliases_availability(&repo, &aliases).await?
        }
    };
    if let Some(warning) = warning {
        return Err(warning);
    }

    let resolved =
        resolve_api_key(&repo, &input.key, kind.default_key_name()).await?;
    let key_id = resolved.id.clone();
    let api_key = resolved.key.clone();
    let created_new_key = resolved.created_new;
    let key_fingerprint = setup_state::fingerprint(&api_key);

    match kind {
        ClientKind::Codex => {
            // 生成 Codex 模型目录，让模型菜单只出现当前启用渠道真正支持的模型。
            let channels = repo
                .get_enabled_channels()
                .await
                .map_err(|err| err.to_string())?;
            let mut catalog_models: Vec<String> =
                crate::server::handlers::collect_models(&channels)
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
                rollback_key(&repo, &key_id, created_new_key).await;
                return Err(format!("生成模型目录失败: {err}"));
            }

            let mut backup_path = existing_record
                .as_ref()
                .and_then(|record| record.backup_path.clone());
            if existing_record.is_none() && paths.config.exists() {
                match setup_state::backup_file(&paths.config, &app_data, "codex-config") {
                    Ok(path) => backup_path = Some(path.to_string_lossy().to_string()),
                    Err(err) => {
                        rollback_key(&repo, &key_id, created_new_key).await;
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
                    requires_openai_auth: paths
                        .auth
                        .as_deref()
                        .map(codex::auth_logged_in)
                        .unwrap_or(false),
                },
            ) {
                Ok(text) => text,
                Err(err) => {
                    rollback_key(&repo, &key_id, created_new_key).await;
                    return Err(err);
                }
            };

            if let Err(err) = setup_state::atomic_write(&paths.config, merged.as_bytes(), 0o600) {
                rollback_key(&repo, &key_id, created_new_key).await;
                return Err(err);
            }

            let previous = existing_record
                .as_ref()
                .map(|record| record.previous.clone())
                .unwrap_or_else(|| codex::capture(&existing_text));

            let record = setup_state::ClientRecord {
                applied_at: chrono::Utc::now().to_rfc3339(),
                config_home: current_home,
                config_path: paths.config.to_string_lossy().to_string(),
                provider_id: codex::PROVIDER_ID.to_string(),
                base_url: gateway.clone(),
                model: model.clone(),
                api_key_id: key_id,
                key_fingerprint,
                previous,
                claude: None,
                backup_path: backup_path.clone(),
                key_managed: created_new_key,
                key_name: Some(resolved.name.clone()),
                key_allowed_models: resolved.allowed_models.clone(),
                key_quota_limit: resolved.quota_limit,
                key_revoked: false,
                last_reconciled_at: None,
            };
            store
                .clients
                .insert(ClientKind::Codex.as_str().to_string(), record);

            if let Err(err) = setup_state::save_store(&app_data, &store) {
                return Err(format!("config.toml 已写入，但保存接入记录失败: {err}"));
            }

            let record = store.clients.get(ClientKind::Codex.as_str());
            let mut status = status_for(kind, &paths, record, &gateway);
            status.applied_api_key_valid = true;

            Ok(ApplyClientResult {
                status,
                backup_path,
                restart_required: true,
                suggest_keep_alive: suggest_keep_alive(&app),
            })
        }
        ClientKind::Claude => {
            let previous_claude = existing_record
                .as_ref()
                .and_then(|record| record.claude.clone());
            let previous = previous_claude
                .as_ref()
                .map(|claude| claude.previous.clone())
                .unwrap_or_else(|| claude_code::capture(&existing_text));

            let mut backup_path = existing_record
                .as_ref()
                .and_then(|record| record.backup_path.clone());
            if existing_record.is_none() && paths.config.exists() && options.apply_cli {
                match setup_state::backup_file(&paths.config, &app_data, "claude-settings") {
                    Ok(path) => backup_path = Some(path.to_string_lossy().to_string()),
                    Err(err) => {
                        rollback_key(&repo, &key_id, created_new_key).await;
                        return Err(err);
                    }
                }
            }

            // 首次接入前的路由别名快照；重复接入时保留最早的值。
            let previous_aliases = match previous_claude
                .as_ref()
                .and_then(|claude| claude.desktop.as_ref())
            {
                Some(desktop) => desktop.previous_aliases.clone(),
                None => read_desktop_aliases(&app),
            };

            let desktop_outcome = if options.apply_desktop {
                let Some(desktop_dir) = paths::resolve_claude_desktop_dir() else {
                    rollback_key(&repo, &key_id, created_new_key).await;
                    return Err(
                        "未检测到 Claude 桌面端第三方推理配置。请先在桌面端开启 Developer Mode 并配置一次后重试，或只勾选 CLI 接入。"
                            .to_string(),
                    );
                };
                match claude_desktop::apply(claude_desktop::ApplyInput {
                    dir: &desktop_dir,
                    spec: &claude_desktop::DesktopSpec {
                        base_url: &gateway,
                        api_key: &api_key,
                        models: &aliases,
                    },
                    app_data: &app_data,
                    existing: previous_claude
                        .as_ref()
                        .and_then(|claude| claude.desktop.as_ref()),
                }) {
                    Ok(mut outcome) => {
                        if let Err(err) =
                            write_desktop_aliases(&app, claude_desktop::alias_map(&aliases))
                        {
                            let _ = claude_desktop::disconnect(&outcome.record, &app_data);
                            rollback_key(&repo, &key_id, created_new_key).await;
                            return Err(err);
                        }
                        outcome.record.previous_aliases = previous_aliases.clone();
                        Some(outcome)
                    }
                    Err(err) => {
                        rollback_key(&repo, &key_id, created_new_key).await;
                        return Err(format!("写入 Claude 桌面端配置失败: {err}"));
                    }
                }
            } else {
                None
            };

            if options.apply_cli {
                let merged = match claude_code::merge_settings(
                    &existing_text,
                    &claude_code::SettingsSpec {
                        base_url: &gateway,
                        api_key: &api_key,
                        models: &aliases,
                    },
                ) {
                    Ok(text) => text,
                    Err(err) => {
                        rollback_desktop(&app, &desktop_outcome, &app_data, &previous_aliases)
                            .await;
                        rollback_key(&repo, &key_id, created_new_key).await;
                        return Err(err);
                    }
                };
                if let Err(err) = setup_state::atomic_write_secret(&paths.config, merged.as_bytes())
                {
                    rollback_desktop(&app, &desktop_outcome, &app_data, &previous_aliases).await;
                    rollback_key(&repo, &key_id, created_new_key).await;
                    return Err(err);
                }
            }

            let desktop_record = desktop_outcome
                .as_ref()
                .map(|outcome| outcome.record.clone())
                .or_else(|| {
                    previous_claude
                        .as_ref()
                        .and_then(|claude| claude.desktop.clone())
                });
            // 已经接入过 CLI 时，即使本次只重新接入桌面端，也要保留 CLI 的清理信息。
            let cli_applied = options.apply_cli
                || previous_claude
                    .as_ref()
                    .is_some_and(|claude| claude.cli_applied);

            let record = setup_state::ClientRecord {
                applied_at: chrono::Utc::now().to_rfc3339(),
                config_home: current_home,
                config_path: paths.config.to_string_lossy().to_string(),
                provider_id: String::new(),
                base_url: gateway.clone(),
                model: model.clone(),
                api_key_id: key_id,
                key_fingerprint,
                previous: Default::default(),
                claude: Some(setup_state::ClaudeRecordData {
                    previous,
                    applied_models: aliases,
                    cli_applied,
                    desktop: desktop_record,
                }),
                backup_path: backup_path.clone(),
                key_managed: created_new_key,
                key_name: Some(resolved.name.clone()),
                key_allowed_models: resolved.allowed_models.clone(),
                key_quota_limit: resolved.quota_limit,
                key_revoked: false,
                last_reconciled_at: None,
            };
            store
                .clients
                .insert(ClientKind::Claude.as_str().to_string(), record);

            if let Err(err) = setup_state::save_store(&app_data, &store) {
                return Err(format!("配置已写入，但保存接入记录失败: {err}"));
            }

            let record = store.clients.get(ClientKind::Claude.as_str());
            let mut status = status_for(kind, &paths, record, &gateway);
            status.applied_api_key_valid = true;

            Ok(ApplyClientResult {
                status,
                backup_path,
                restart_required: true,
                suggest_keep_alive: suggest_keep_alive(&app),
            })
        }
    }
}

/// 接入后只提示一次常驻建议；用户已关闭或已开启自启/托盘则不再打扰。
fn suggest_keep_alive(app: &tauri::AppHandle) -> bool {
    if client_setup::keep_alive_prompted(app) {
        return false;
    }
    let Ok(store) = app.store("settings.json") else {
        return false;
    };
    let auto_start = store
        .get("ui.auto_start")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let close_to_tray = store
        .get("ui.close_to_tray")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    !auto_start || !close_to_tray
}

async fn rollback_desktop(
    app: &tauri::AppHandle,
    outcome: &Option<claude_desktop::ApplyOutcome>,
    app_data: &std::path::Path,
    previous_aliases: &Option<serde_json::Value>,
) {
    if let Some(outcome) = outcome {
        let _ = claude_desktop::disconnect(&outcome.record, app_data);
        let _ = restore_desktop_aliases(app, previous_aliases);
    }
}
#[tauri::command]
pub async fn client_setup_disconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<DisconnectClientResult, String> {
    let kind = ClientKind::parse(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let paths = client_setup::resolve_paths(kind)?;

    let _guard = client_setup::SETUP_LOCK.lock().await;

    let mut store = setup_state::load_store(&app_data)?;
    let record = store
        .clients
        .get(kind.as_str())
        .cloned()
        .ok_or_else(|| format!("未找到 {} 接入记录", kind.display_name()))?;

    let existing = setup_state::read_optional(&paths.config)?.unwrap_or_default();
    let mut warnings = Vec::new();

    match kind {
        ClientKind::Codex => {
            if existing.trim().is_empty() {
                warnings.push("config.toml 不存在，仅清除了接入记录".to_string());
                store.clients.remove(ClientKind::Codex.as_str());
            } else {
                let (restored, output) = codex::disconnect_config(&existing, &record)?;
                if output.remove_config_file {
                    std::fs::remove_file(&paths.config)
                        .map_err(|err| format!("删除 {} 失败: {err}", paths.config.display()))?;
                } else {
                    setup_state::atomic_write(&paths.config, restored.as_bytes(), 0o600)?;
                }
                warnings.extend(output.warnings);
                store.clients.remove(ClientKind::Codex.as_str());
            }
        }
        ClientKind::Claude => {
            let cli_applied = record
                .claude
                .as_ref()
                .map(|claude| claude.cli_applied)
                .unwrap_or(true);
            if cli_applied {
                if existing.trim().is_empty() {
                    warnings.push("settings.json 不存在，未恢复 CLI 配置".to_string());
                } else {
                    let (restored, output) = claude_code::disconnect_settings(&existing, &record)?;
                    if output.remove_settings_file {
                        std::fs::remove_file(&paths.config).map_err(|err| {
                            format!("删除 {} 失败: {err}", paths.config.display())
                        })?;
                    } else {
                        setup_state::atomic_write_secret(&paths.config, restored.as_bytes())?;
                    }
                    warnings.extend(output.warnings);
                }
            }
            if let Some(desktop) = record
                .claude
                .as_ref()
                .and_then(|claude| claude.desktop.as_ref())
            {
                match claude_desktop::disconnect(desktop, &app_data) {
                    Ok(mut desktop_warnings) => warnings.append(&mut desktop_warnings),
                    Err(err) => warnings.push(format!("恢复 Claude 桌面端配置失败: {err}")),
                }
                if let Err(err) = restore_desktop_aliases(&app, &desktop.previous_aliases) {
                    warnings.push(err);
                }
            }
            store.clients.remove(ClientKind::Claude.as_str());
        }
    }

    setup_state::save_store(&app_data, &store)?;

    let gateway = gateway_for(kind, &app, state.inner()).await;
    Ok(DisconnectClientResult {
        status: status_for(kind, &paths, None, &gateway),
        backup_path: record.backup_path.clone(),
        warnings,
    })
}

#[tauri::command]
pub async fn client_setup_test(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: String,
) -> Result<ClientTestResult, String> {
    let kind = ClientKind::parse(&client)?;

    let app_data = client_setup::app_data_dir(&app)?;
    let store = setup_state::load_store(&app_data)?;
    let record = store
        .clients
        .get(kind.as_str())
        .cloned()
        .ok_or_else(|| format!("请先完成 {} 接入", kind.display_name()))?;

    let repo = Repository::new(state.db.pool.clone());
    let aliases = record
        .claude
        .as_ref()
        .map(|claude| claude.applied_models.clone())
        .unwrap_or_else(|| ClaudeModelAliases::resolve(&record.model, None));
    let warning = match kind {
        ClientKind::Codex => model_availability(&repo, &record.model).await?,
        ClientKind::Claude => aliases_availability(&repo, &aliases).await?,
    };
    if let Some(warning) = warning {
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

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;
    let started = std::time::Instant::now();

    let (status, text) = match kind {
        ClientKind::Codex => {
            let url = format!("{}/responses", record.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": record.model,
                "input": "回复 OK 两个字母即可",
                "max_output_tokens": 32,
                "stream": false,
            });
            let response = http
                .post(&url)
                .bearer_auth(&key.key)
                .json(&body)
                .send()
                .await
                .map_err(|err| format!("请求网关失败: {err}"))?;
            (
                response.status().as_u16(),
                response.text().await.unwrap_or_default(),
            )
        }
        ClientKind::Claude => {
            let url = format!("{}/v1/messages", record.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": aliases.main,
                "max_tokens": 32,
                "messages": [{ "role": "user", "content": "回复 OK 两个字母即可" }],
                "stream": false,
            });
            let response = http
                .post(&url)
                .bearer_auth(&key.key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|err| format!("请求网关失败: {err}"))?;
            (
                response.status().as_u16(),
                response.text().await.unwrap_or_default(),
            )
        }
    };

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
pub fn client_setup_open_dir(app: tauri::AppHandle, client: String) -> Result<(), String> {    use tauri_plugin_opener::OpenerExt;

    let kind = ClientKind::parse(&client)?;
    let paths = client_setup::resolve_paths(kind)?;
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

/// 立即自检 / 修复。`mode = "repair"` 允许轮换托管 Key；默认 Auto。
#[tauri::command]
pub async fn client_setup_reconcile(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    client: Option<String>,
    mode: Option<String>,
) -> Result<reconcile::ReconcileReport, String> {
    let repair_mode = match mode.as_deref() {
        Some("repair") => reconcile::RepairMode::Repair,
        _ => reconcile::RepairMode::Auto,
    };

    let report = match client.as_deref() {
        Some(raw) => {
            let kind = ClientKind::parse(raw)?;
            let result = reconcile::reconcile_one(&app, state.inner(), kind, repair_mode).await;
            let gateway = client_setup::gateway_base_url(&app, state.inner()).await;
            let gateway_ok = state
                .server_running
                .load(std::sync::atomic::Ordering::SeqCst);
            let mut clients = std::collections::BTreeMap::new();
            clients.insert(kind.as_str().to_string(), result);
            reconcile::ReconcileReport {
                ran_at: chrono::Utc::now().to_rfc3339(),
                gateway,
                gateway_ok,
                clients,
            }
        }
        None => reconcile::reconcile_all(&app, state.inner(), repair_mode).await,
    };

    *state.reconcile_report.write().await = Some(report.clone());
    Ok(report)
}

#[tauri::command]
pub async fn client_setup_last_report(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<reconcile::ReconcileReport>, String> {
    Ok(state.reconcile_report.read().await.clone())
}

#[tauri::command]
pub fn client_setup_dismiss_keep_alive_hint(app: tauri::AppHandle) -> Result<(), String> {
    client_setup::mark_keep_alive_prompted(&app)
}

fn extract_response_text(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    // Anthropic Messages: content[].text
    if let Some(content) = value.get("content").and_then(|item| item.as_array()) {
        for part in content {
            if let Some(text) = part.get("text").and_then(|part| part.as_str()) {
                if !text.trim().is_empty() {
                    return Some(text.trim().to_string());
                }
            }
        }
    }
    // OpenAI Responses: output[].content[].text / output_text
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
