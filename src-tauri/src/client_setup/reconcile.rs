//! 客户端接入自检与自动修复。
//!
//! 原则：先判「这份配置是否仍由 YeAPI 写入并持有」（只看 token 指纹 / provider /
//! 桌面端 appliedId，不看 base_url），只修复自己的配置；被 cc-switch 等外部工具
//! 改写时只报告冲突，绝不自动覆盖。

use crate::client_setup::state::{self as setup_state, ClientRecord};
use crate::client_setup::{
    self, claude_code, claude_desktop, codex, ClientIssue, ClientKind, ClientPaths,
    ClaudeModelAliases,
};
use crate::db::models::{ApiKey, CreateApiKeyInput};
use crate::db::repository::Repository;
use crate::server::auth::key_is_expired;
use crate::AppState;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairMode {
    /// 启动/重启/渠道变化时触发，受设置开关约束。
    Auto,
    /// 用户主动点「重新接管/立即修复」，允许轮换 Key；仍不覆盖外部冲突。
    Repair,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientReconcileResult {
    pub client: String,
    /// applied | repaired | conflict | needs_action | not_applied | error
    pub state: String,
    pub actions: Vec<String>,
    pub issues: Vec<ClientIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileReport {
    pub ran_at: String,
    pub gateway: String,
    pub gateway_ok: bool,
    pub clients: BTreeMap<String, ClientReconcileResult>,
}

#[derive(Debug, Clone, Copy)]
struct RepairSettings {
    auto_repair: bool,
    auto_rotate_keys: bool,
    respect_external: bool,
}

fn repair_settings(app: &AppHandle) -> RepairSettings {
    let get = |key: &str| {
        app.store("settings.json")
            .ok()
            .and_then(|store| store.get(key))
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    };
    RepairSettings {
        auto_repair: get("client_setup.auto_repair"),
        auto_rotate_keys: get("client_setup.auto_rotate_keys"),
        respect_external: get("client_setup.respect_external"),
    }
}

/// 只读检查结果，供 `client_setup_status` 直接渲染，也供 reconcile 决策。
pub struct Inspection {
    pub paths: ClientPaths,
    pub owned: bool,
    pub key_state: String,
    pub external_conflict: bool,
    pub catalog_stale: bool,
    pub current_base_url: Option<String>,
    pub issues: Vec<ClientIssue>,
}

fn issue(
    code: &str,
    severity: &str,
    message: impl Into<String>,
    auto_fixable: bool,
    requires_confirm: bool,
) -> ClientIssue {
    ClientIssue {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.into(),
        auto_fixable,
        requires_confirm,
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 记录里保存的配置路径优先于当前进程环境变量，避免 CODEX_HOME /
/// CLAUDE_CONFIG_DIR 变化后「找不到自己的配置」。
fn effective_paths(kind: ClientKind, record: &ClientRecord) -> ClientPaths {
    let home = PathBuf::from(&record.config_home);
    let config = if record.config_path.trim().is_empty() {
        match kind {
            ClientKind::Codex => home.join("config.toml"),
            ClientKind::Claude => home.join("settings.json"),
        }
    } else {
        PathBuf::from(&record.config_path)
    };
    let auth = matches!(kind, ClientKind::Codex).then(|| home.join("auth.json"));
    ClientPaths { home, config, auth }
}

fn codex_provider_item(doc: &DocumentMut) -> Option<&dyn toml_edit::TableLike> {
    doc.get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(codex::PROVIDER_ID))
        .and_then(Item::as_table_like)
}

fn codex_current_base_url(text: &str) -> Option<String> {
    let doc: DocumentMut = text.parse().ok()?;
    codex_provider_item(&doc)
        .and_then(|provider| provider.get("base_url"))
        .and_then(Item::as_str)
        .map(str::to_string)
}

fn codex_owned(text: &str, key_fingerprint: &str) -> bool {
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return false;
    };
    let provider_matches =
        doc.get("model_provider").and_then(Item::as_str) == Some(codex::PROVIDER_ID);
    let token_matches = codex_provider_item(&doc)
        .and_then(|provider| provider.get("experimental_bearer_token"))
        .and_then(Item::as_str)
        .map(setup_state::fingerprint)
        .as_deref()
        == Some(key_fingerprint);
    provider_matches && token_matches
}

fn claude_env(text: &str) -> Option<serde_json::Map<String, Value>> {
    let value: Value = serde_json::from_str(text).ok()?;
    match value.get("env") {
        Some(Value::Object(map)) => Some(map.clone()),
        _ => None,
    }
}

fn claude_current_base_url(text: &str) -> Option<String> {
    claude_env(text)?
        .get(claude_code::BASE_URL_KEY)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn claude_owned(text: &str, key_fingerprint: &str) -> bool {
    claude_env(text)
        .and_then(|env| {
            env.get(claude_code::AUTH_TOKEN_KEY)
                .and_then(Value::as_str)
                .map(setup_state::fingerprint)
        })
        .as_deref()
        == Some(key_fingerprint)
}

fn desktop_applied(record: &ClientRecord) -> Option<bool> {
    let desktop = record
        .claude
        .as_ref()
        .and_then(|claude| claude.desktop.as_ref())?;
    let meta_text = std::fs::read_to_string(&desktop.meta_path).ok()?;
    let meta: Value = serde_json::from_str(&meta_text).ok()?;
    Some(
        meta.get("appliedId").and_then(Value::as_str)
            == Some(desktop.profile_id.as_str()),
    )
}

fn desktop_current_base_url(record: &ClientRecord) -> Option<String> {
    let desktop = record
        .claude
        .as_ref()
        .and_then(|claude| claude.desktop.as_ref())?;
    let text = std::fs::read_to_string(&desktop.profile_path).ok()?;
    let profile: Value = serde_json::from_str(&text).ok()?;
    profile
        .get("inferenceGatewayBaseUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
}

async fn catalog_models(repo: &Repository, current_model: &str) -> Vec<String> {
    let channels = repo.get_enabled_channels().await.unwrap_or_default();
    let mut models: Vec<String> = crate::server::handlers::collect_models(&channels)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    models.sort();
    models.dedup();
    if !models
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(current_model))
    {
        models.push(current_model.to_string());
    }
    models
}

fn catalog_matches(home: &Path, expected: &[String]) -> bool {
    let path = home.join(codex::CATALOG_FILENAME);
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let mut slugs: Vec<String> = value
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("slug").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    slugs.sort();
    slugs.dedup();
    let mut wanted = expected.to_vec();
    wanted.sort();
    wanted.dedup();
    slugs == wanted
}

async fn key_state(repo: &Repository, record: &ClientRecord) -> (String, Option<ApiKey>) {
    if record.key_revoked {
        return ("revoked".to_string(), None);
    }
    let keys = repo.get_all_api_keys().await.unwrap_or_default();
    match keys.iter().find(|key| key.id == record.api_key_id) {
        None => ("missing".to_string(), None),
        Some(key) if key.status != 1 => ("disabled".to_string(), Some(key.clone())),
        Some(key)
            if key
                .expires_at
                .as_deref()
                .is_some_and(|expiry| key_is_expired(expiry)) =>
        {
            ("expired".to_string(), Some(key.clone()))
        }
        Some(key) => ("ok".to_string(), Some(key.clone())),
    }
}

fn key_issue(record: &ClientRecord, key_state: &str) -> Option<ClientIssue> {
    let managed = record.key_managed && !record.key_revoked;
    match key_state {
        "missing" => Some(issue(
            "key_missing",
            "danger",
            if managed {
                "接入使用的托管密钥已丢失，可自动创建新密钥并更新配置"
            } else {
                "接入使用的密钥已被删除，请重新接入"
            },
            managed,
            !managed,
        )),
        "expired" => Some(issue(
            "key_expired",
            "warning",
            if managed {
                "接入使用的托管密钥已过期，可自动轮换并更新配置"
            } else {
                "接入使用的密钥已过期，请重新接入或更换密钥"
            },
            managed,
            !managed,
        )),
        "disabled" => Some(issue(
            "key_disabled",
            "warning",
            "接入使用的密钥已被禁用，请重新启用或重新接入",
            false,
            true,
        )),
        "revoked" => Some(issue(
            "key_revoked",
            "warning",
            "接入使用的密钥已被你删除，自检不会自动重建，请重新接入",
            false,
            true,
        )),
        _ => None,
    }
}

/// 只读检查：供状态接口展示与 reconcile 决策。
pub async fn inspect(
    app: &AppHandle,
    state: &Arc<AppState>,
    kind: ClientKind,
    record: Option<&ClientRecord>,
) -> Inspection {
    let fallback_paths = client_setup::resolve_paths(kind).unwrap_or(ClientPaths {
        home: PathBuf::new(),
        config: PathBuf::new(),
        auth: None,
    });
    let Some(record) = record else {
        return Inspection {
            paths: fallback_paths,
            owned: false,
            key_state: "unknown".to_string(),
            external_conflict: false,
            catalog_stale: false,
            current_base_url: None,
            issues: Vec::new(),
        };
    };

    let paths = effective_paths(kind, record);
    let gateway_codex = client_setup::gateway_base_url(app, state).await;
    let gateway_origin = client_setup::gateway_origin(app, state).await;
    let gateway_ok = state.server_running.load(Ordering::SeqCst)
        && *state.server_port.read().await != 0;
    let desktop_dir = client_setup::paths::resolve_claude_desktop_dir();
    let repo = Repository::new(state.db.pool.clone());
    let settings = repair_settings(app);
    let (key_state, _) = key_state(&repo, record).await;

    let mut issues = Vec::new();
    let mut owned = true;
    let mut external_conflict = false;
    let mut catalog_stale = false;
    let mut current_base_url = None;

    match kind {
        ClientKind::Codex => {
            let text = setup_state::read_optional(&paths.config).ok().flatten();
            match text.as_deref() {
                None | Some("") => {
                    if paths.home.exists() {
                        issues.push(issue(
                            "config_missing",
                            "warning",
                            format!("未找到 {}，可一键恢复接入", paths.config.display()),
                            true,
                            false,
                        ));
                    }
                    owned = false;
                }
                Some(text) => match text.parse::<DocumentMut>() {
                    Err(err) => {
                        owned = false;
                        issues.push(issue(
                            "parse_error",
                            "danger",
                            format!("config.toml 解析失败: {err}"),
                            false,
                            true,
                        ));
                    }
                    Ok(_) => {
                        current_base_url = codex_current_base_url(text);
                        owned = codex_owned(text, &record.key_fingerprint);
                        if !owned {
                            external_conflict = true;
                            issues.push(issue(
                                "external_conflict",
                                "danger",
                                "Codex 配置已被其他工具（如 cc-switch）改写。如需接管，请点「重新接入」（会先备份原配置）",
                                false,
                                true,
                            ));
                        } else if current_base_url.as_deref() != Some(gateway_codex.as_str()) {
                            issues.push(issue(
                                "gateway_changed",
                                "warning",
                                format!("网关地址已变为 {gateway_codex}，可自动更新 Codex 配置"),
                                gateway_ok,
                                false,
                            ));
                        }
                    }
                },
            }

            let expected = catalog_models(&repo, &record.model).await;
            catalog_stale = !catalog_matches(&paths.home, &expected);
            if catalog_stale && owned {
                issues.push(issue(
                    "catalog_stale",
                    "info",
                    "Codex 模型目录缺失或与当前启用渠道不一致，可自动重建",
                    true,
                    false,
                ));
            }
        }
        ClientKind::Claude => {
            let cli_applied = record
                .claude
                .as_ref()
                .map(|claude| claude.cli_applied)
                .unwrap_or(true);
            if cli_applied {
                let text = setup_state::read_optional(&paths.config).ok().flatten();
                match text.as_deref() {
                    None | Some("") => {
                        if paths.home.exists() {
                            issues.push(issue(
                                "config_missing",
                                "warning",
                                format!("未找到 {}，可一键恢复 CLI 接入", paths.config.display()),
                                true,
                                false,
                            ));
                        }
                        owned = false;
                    }
                    Some(text) => match serde_json::from_str::<Value>(text) {
                        Err(err) => {
                            owned = false;
                            issues.push(issue(
                                "parse_error",
                                "danger",
                                format!("settings.json 解析失败: {err}"),
                                false,
                                true,
                            ));
                        }
                        Ok(_) => {
                            let base_url = claude_current_base_url(text);
                            current_base_url = base_url.clone();
                            if !claude_owned(text, &record.key_fingerprint) {
                                owned = false;
                                external_conflict = true;
                                issues.push(issue(
                                    "external_conflict",
                                    "danger",
                                    "Claude Code 配置已被其他工具（如 cc-switch）改写。如需接管，请点「重新接入」（会先备份原配置）",
                                    false,
                                    true,
                                ));
                            } else if base_url.as_deref() != Some(gateway_origin.as_str()) {
                                issues.push(issue(
                                    "gateway_changed",
                                    "warning",
                                    format!("网关地址已变为 {gateway_origin}，可自动更新 CLI 配置"),
                                    gateway_ok,
                                    false,
                                ));
                            }
                        }
                    },
                }
            }

            if let Some(desktop) = record
                .claude
                .as_ref()
                .and_then(|claude| claude.desktop.as_ref())
            {
                let meta_exists = Path::new(&desktop.meta_path).exists();
                if meta_exists {
                    let applied = desktop_applied(record).unwrap_or(false);
                    if !applied {
                        owned = false;
                        external_conflict = true;
                        issues.push(issue(
                            "desktop_not_active",
                            "danger",
                            "Claude 桌面端已切换到其他推理档案（如 CC Switch）。如需接管，请重新接入桌面端（会先备份原配置）",
                            false,
                            true,
                        ));
                    } else {
                        let base_url = desktop_current_base_url(record);
                        if base_url.as_deref() != Some(gateway_origin.as_str()) {
                            issues.push(issue(
                                "gateway_changed",
                                "warning",
                                format!(
                                    "网关地址已变为 {gateway_origin}，可自动更新桌面端配置"
                                ),
                                gateway_ok,
                                false,
                            ));
                        }
                    }
                } else if desktop_dir.is_some() {
                    issues.push(issue(
                        "desktop_not_active",
                        "warning",
                        "Claude 桌面端推理档案不存在，可重新接入桌面端",
                        false,
                        true,
                    ));
                }
            }
        }
    }

    if !gateway_ok {
        issues.push(issue(
            "gateway_down",
            "warning",
            "本地网关当前未运行，客户端请求会失败；请检查端口占用或点击重试",
            false,
            true,
        ));
    }

    if let Some(key_issue) = key_issue(record, &key_state) {
        let auto_fixable = key_issue.auto_fixable && settings.auto_rotate_keys;
        issues.push(ClientIssue {
            auto_fixable,
            ..key_issue
        });
    }

    if let Ok(current) = client_setup::resolve_paths(kind) {
        if current.home != paths.home {
            issues.push(issue(
                "path_mismatch",
                "info",
                format!(
                    "当前环境指向 {}，自检将使用接入记录中的 {}",
                    current.home.display(),
                    paths.home.display()
                ),
                false,
                false,
            ));
        }
    }

    Inspection {
        paths,
        owned,
        key_state,
        external_conflict,
        catalog_stale,
        current_base_url,
        issues,
    }
}

pub async fn reconcile_all(
    app: &AppHandle,
    state: &Arc<AppState>,
    mode: RepairMode,
) -> ReconcileReport {
    let gateway_codex = client_setup::gateway_base_url(app, state).await;
    let gateway_ok =
        state.server_running.load(Ordering::SeqCst) && *state.server_port.read().await != 0;
    let mut clients = BTreeMap::new();

    for kind in [ClientKind::Codex, ClientKind::Claude] {
        let result = match reconcile_client(app, state, kind, mode).await {
            Ok(result) => result,
            Err(err) => ClientReconcileResult {
                client: kind.as_str().to_string(),
                state: "error".to_string(),
                actions: Vec::new(),
                issues: vec![issue("reconcile_error", "danger", err, false, true)],
            },
        };
        clients.insert(kind.as_str().to_string(), result);
    }

    let report = ReconcileReport {
        ran_at: now_iso(),
        gateway: gateway_codex,
        gateway_ok,
        clients,
    };
    app.emit(
        "client-setup-reconciled",
        &report,
    )
    .ok();
    tracing::info!(
        "client setup reconcile finished (gateway={}, ok={})",
        report.gateway,
        gateway_ok
    );
    report
}

async fn reconcile_client(
    app: &AppHandle,
    state: &Arc<AppState>,
    kind: ClientKind,
    mode: RepairMode,
) -> Result<ClientReconcileResult, String> {
    // 与手动 apply / disconnect 串行化，避免并发写坏同一份配置。
    let _guard = client_setup::SETUP_LOCK.lock().await;
    let app_data = client_setup::app_data_dir(app)?;
    let mut store = setup_state::load_store(&app_data)?;
    let Some(record) = store.clients.get(kind.as_str()).cloned() else {
        return Ok(ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: "not_applied".to_string(),
            actions: Vec::new(),
            issues: Vec::new(),
        });
    };

    let inspection = inspect(app, state, kind, Some(&record)).await;
    let settings = repair_settings(app);

    if settings.respect_external && inspection.external_conflict {
        return Ok(ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: "conflict".to_string(),
            actions: Vec::new(),
            issues: inspection.issues,
        });
    }

    let allow_auto = matches!(mode, RepairMode::Repair) || settings.auto_repair;
    if !allow_auto {
        let state_label = if inspection.issues.is_empty() {
            "applied"
        } else {
            "needs_action"
        };
        return Ok(ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: state_label.to_string(),
            actions: Vec::new(),
            issues: inspection.issues,
        });
    }

    // 解析失败时不做任何写入，避免覆盖一个我们看不懂的配置。
    if inspection
        .issues
        .iter()
        .any(|issue| issue.code == "parse_error")
    {
        return Ok(ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: "needs_action".to_string(),
            actions: Vec::new(),
            issues: inspection.issues,
        });
    }

    let gateway_codex = client_setup::gateway_base_url(app, state).await;
    let gateway_origin = client_setup::gateway_origin(app, state).await;
    let gateway_ok =
        state.server_running.load(Ordering::SeqCst) && *state.server_port.read().await != 0;
    let repo = Repository::new(state.db.pool.clone());

    let mut actions = Vec::new();
    let mut record = record;
    // 轮换 Key 后 record 的指纹会变；写前校验必须用「配置里旧 Key」的指纹。
    let ownership_fingerprint = record.key_fingerprint.clone();
    let (current_key_state, current_key) = key_state(&repo, &record).await;

    let can_rotate = record.key_managed
        && !record.key_revoked
        && (settings.auto_rotate_keys || matches!(mode, RepairMode::Repair))
        && matches!(current_key_state.as_str(), "missing" | "expired");

    let mut rotated_key_id: Option<String> = None;
    let api_key = if can_rotate {
        let created = repo
            .create_api_key(&CreateApiKeyInput {
                name: record
                    .key_name
                    .clone()
                    .unwrap_or_else(|| kind.display_name().to_string()),
                allowed_models: record.key_allowed_models.clone(),
                allowed_channels: Vec::new(),
                quota_limit: record.key_quota_limit,
                expires_at: None,
            })
            .await
            .map_err(|err| format!("自动创建密钥失败: {err}"))?;
        record.api_key_id = created.id.clone();
        record.key_fingerprint = setup_state::fingerprint(&created.key);
        record.key_revoked = false;
        rotated_key_id = Some(created.id.clone());
        actions.push("已为失效的托管密钥创建新密钥".to_string());
        created.key
    } else if let Some(key) = current_key.filter(|_| current_key_state == "ok") {
        key.key
    } else {
        // 没有可用 Key 且不可自动轮换：保留现状，交给用户处理。
        return Ok(ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: "needs_action".to_string(),
            actions,
            issues: inspection.issues,
        });
    };

    let write_result = perform_writes(
        app,
        kind,
        &repo,
        &mut record,
        &api_key,
        &ownership_fingerprint,
        rotated_key_id.is_some(),
        gateway_ok,
        &gateway_codex,
        &gateway_origin,
        &inspection,
        &app_data,
    )
    .await;

    let (written_actions, wrote) = match write_result {
        Ok(outcome) => outcome,
        Err(err) => {
            if let Some(id) = &rotated_key_id {
                let _ = repo.delete_api_key(id).await;
            }
            return Err(err);
        }
    };
    actions.extend(written_actions);

    if rotated_key_id.is_some() || wrote {
        record.last_reconciled_at = Some(now_iso());
        store
            .clients
            .insert(kind.as_str().to_string(), record.clone());
        setup_state::save_store(&app_data, &store)?;
    }

    let state_label = if !actions.is_empty() {
        "repaired"
    } else if inspection.issues.is_empty() {
        "applied"
    } else {
        "needs_action"
    };

    Ok(ClientReconcileResult {
        client: kind.as_str().to_string(),
        state: state_label.to_string(),
        actions,
        issues: inspection.issues,
    })
}

/// 单个客户端的自检结果（命令层用于「立即修复」）。
pub async fn reconcile_one(
    app: &AppHandle,
    state: &Arc<AppState>,
    kind: ClientKind,
    mode: RepairMode,
) -> ClientReconcileResult {
    match reconcile_client(app, state, kind, mode).await {
        Ok(result) => result,
        Err(err) => ClientReconcileResult {
            client: kind.as_str().to_string(),
            state: "error".to_string(),
            actions: Vec::new(),
            issues: vec![issue("reconcile_error", "danger", err, false, true)],
        },
    }
}

async fn perform_writes(
    app: &AppHandle,
    kind: ClientKind,
    repo: &Repository,
    record: &mut ClientRecord,
    api_key: &str,
    ownership_fingerprint: &str,
    key_rotated: bool,
    gateway_ok: bool,
    gateway_codex: &str,
    gateway_origin: &str,
    inspection: &Inspection,
    app_data: &Path,
) -> Result<(Vec<String>, bool), String> {
    let target_base = |gateway: &str| -> String {
        if gateway_ok {
            gateway.to_string()
        } else {
            inspection
                .current_base_url
                .clone()
                .unwrap_or_else(|| gateway.to_string())
        }
    };

    let address_changed = gateway_ok
        && inspection
            .issues
            .iter()
            .any(|issue| issue.code == "gateway_changed");
    let config_missing = inspection
        .issues
        .iter()
        .any(|issue| issue.code == "config_missing");

    let mut actions = Vec::new();
    let mut wrote = false;

    match kind {
        ClientKind::Codex => {
            let existing_text =
                setup_state::read_optional(&inspection.paths.config)?.unwrap_or_default();
            let owned_now = existing_text.is_empty() || codex_owned(&existing_text, ownership_fingerprint);
            if !owned_now {
                return Err("配置在自检期间被其他工具改写，已跳过自动修复".to_string());
            }

            let need_write = key_rotated || (gateway_ok && (address_changed || config_missing));
            let need_catalog = inspection.catalog_stale && gateway_ok;

            if need_write || need_catalog {
                if !existing_text.is_empty() {
                    setup_state::backup_file(
                        &inspection.paths.config,
                        app_data,
                        "codex-config-auto",
                    )?;
                }

                if need_write {
                    let base_url = target_base(gateway_codex);
                    let merged = codex::merge_config(
                        &existing_text,
                        &codex::ProviderSpec {
                            base_url: &base_url,
                            model: &record.model,
                            api_key,
                            requires_openai_auth: inspection
                                .paths
                                .auth
                                .as_deref()
                                .map(codex::auth_logged_in)
                                .unwrap_or(false),
                        },
                    )?;
                    setup_state::atomic_write(&inspection.paths.config, merged.as_bytes(), 0o600)?;
                    record.base_url = base_url;
                    wrote = true;
                    actions.push("已更新 Codex 配置".to_string());
                }

                if gateway_ok {
                    let models = catalog_models(repo, &record.model).await;
                    setup_state::atomic_write(
                        &inspection.paths.home.join(codex::CATALOG_FILENAME),
                        codex::build_model_catalog(&models).as_bytes(),
                        0o644,
                    )?;
                    actions.push("已重建 Codex 模型目录".to_string());
                }
            }
        }
        ClientKind::Claude => {
            let cli_applied = record
                .claude
                .as_ref()
                .map(|claude| claude.cli_applied)
                .unwrap_or(true);
            let aliases = record
                .claude
                .as_ref()
                .map(|claude| claude.applied_models.clone())
                .unwrap_or_else(|| ClaudeModelAliases::resolve(&record.model, None));

            let need_cli_write =
                key_rotated || (gateway_ok && (address_changed || config_missing));

            if cli_applied && need_cli_write {
                let existing_text =
                    setup_state::read_optional(&inspection.paths.config)?.unwrap_or_default();
                let owned_now = existing_text.is_empty() || claude_owned(&existing_text, ownership_fingerprint);
                if !owned_now {
                    return Err("配置在自检期间被其他工具改写，已跳过自动修复".to_string());
                }

                if !existing_text.is_empty() {
                    setup_state::backup_file(
                        &inspection.paths.config,
                        app_data,
                        "claude-settings-auto",
                    )?;
                }
                let base_url = target_base(gateway_origin);
                let merged = claude_code::merge_settings(
                    &existing_text,
                    &claude_code::SettingsSpec {
                        base_url: &base_url,
                        api_key,
                        models: &aliases,
                    },
                )?;
                setup_state::atomic_write_secret(&inspection.paths.config, merged.as_bytes())?;
                record.base_url = base_url;
                wrote = true;
                actions.push("已更新 Claude Code settings.json".to_string());
            }

            if let Some(desktop) = record
                .claude
                .as_ref()
                .and_then(|claude| claude.desktop.clone())
            {
                let applied = desktop_applied(record).unwrap_or(false);
                if applied && (key_rotated || (gateway_ok && address_changed)) {
                    let base_url = target_base(gateway_origin);
                    let outcome = claude_desktop::apply(claude_desktop::ApplyInput {
                        dir: Path::new(&desktop.config_dir),
                        spec: &claude_desktop::DesktopSpec {
                            base_url: &base_url,
                            api_key,
                            models: &aliases,
                        },
                        app_data,
                        existing: Some(&desktop),
                    })?;
                    if let Some(claude) = record.claude.as_mut() {
                        claude.desktop = Some(outcome.record);
                    }
                    write_desktop_aliases(app, claude_desktop::alias_map(&aliases))?;
                    record.base_url = base_url;
                    wrote = true;
                    actions.push("已更新 Claude 桌面端推理档案".to_string());
                }
            }
        }
    }

    Ok((actions, wrote))
}

fn write_desktop_aliases(app: &AppHandle, value: Value) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|err| format!("打开设置存储失败: {err}"))?;
    store.set("claude.desktop_aliases", value);
    store
        .save()
        .map_err(|err| format!("保存模型路由别名失败: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_setup::state::PreviousValues;

    fn record_with(fp: &str) -> ClientRecord {
        ClientRecord {
            applied_at: "now".into(),
            config_home: "/tmp/.codex".into(),
            config_path: "/tmp/.codex/config.toml".into(),
            base_url: "http://127.0.0.1:8777/v1".into(),
            model: "m".into(),
            api_key_id: "id".into(),
            key_fingerprint: fp.into(),
            provider_id: "yeapi".into(),
            previous: PreviousValues::default(),
            claude: None,
            backup_path: None,
            key_managed: true,
            key_name: Some("Codex".into()),
            key_allowed_models: Vec::new(),
            key_quota_limit: Some(-1),
            key_revoked: false,
            last_reconciled_at: None,
        }
    }

    #[test]
    fn codex_ownership_uses_token_fingerprint_only() {
        let token = "sk-yeapi-test";
        let record = record_with(&setup_state::fingerprint(token));
        let text = format!(
            "model_provider = \"yeapi\"\n[model_providers.yeapi]\nbase_url = \"http://127.0.0.1:9999/v1\"\nexperimental_bearer_token = \"{token}\"\n"
        );
        assert!(codex_owned(&text, &record.key_fingerprint));
        assert_eq!(
            codex_current_base_url(&text).as_deref(),
            Some("http://127.0.0.1:9999/v1")
        );

        let other = text.replace("yeapi", "other");
        assert!(!codex_owned(&other, &record.key_fingerprint));
    }

    #[test]
    fn claude_ownership_ignores_base_url() {
        let token = "sk-yeapi-test";
        let record = record_with(&setup_state::fingerprint(token));
        let text = serde_json::json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": token,
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
            }
        })
        .to_string();
        assert!(claude_owned(&text, &record.key_fingerprint));
        assert_eq!(
            claude_current_base_url(&text).as_deref(),
            Some("http://127.0.0.1:15721")
        );
    }

    #[test]
    fn key_issue_respects_managed_and_revoked() {
        let mut record = record_with("fp");
        assert!(key_issue(&record, "missing").unwrap().auto_fixable);
        record.key_revoked = true;
        let revoked = key_issue(&record, "missing").unwrap();
        assert!(!revoked.auto_fixable);
        assert!(revoked.requires_confirm);
        record.key_revoked = false;
        record.key_managed = false;
        assert!(!key_issue(&record, "expired").unwrap().auto_fixable);
    }

    #[test]
    fn effective_paths_prefer_record() {
        let mut record = record_with("fp");
        record.config_home = "/tmp/recorded-codex".into();
        record.config_path = "/tmp/recorded-codex/config.toml".into();
        let paths = effective_paths(ClientKind::Codex, &record);
        assert_eq!(paths.home, PathBuf::from("/tmp/recorded-codex"));
        assert_eq!(
            paths.auth.as_deref(),
            Some(Path::new("/tmp/recorded-codex/auth.json"))
        );
    }

    #[test]
    fn claude_record_can_be_deserialized_from_old_json() {
        let legacy = serde_json::json!({
            "applied_at": "now",
            "config_home": "/tmp/.claude",
            "config_path": "/tmp/.claude/settings.json",
            "base_url": "http://127.0.0.1:8777",
            "model": "m",
            "api_key_id": "id",
            "key_fingerprint": "fp",
            "provider_id": "",
            "previous": {},
            "claude": {
                "previous": {},
                "applied_models": { "main": "m", "opus": "m", "sonnet": "m", "haiku": "m" },
                "cli_applied": true,
                "desktop": null
            },
            "backup_path": null
        });
        let record: ClientRecord = serde_json::from_value(legacy).unwrap();
        assert!(!record.key_managed);
        assert!(!record.key_revoked);
        assert!(record.last_reconciled_at.is_none());
    }
}
