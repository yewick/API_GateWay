//! Codex 配置（`~/.codex/config.toml`）的检测、合并与断开。

use super::state::{self, ClientRecord, PreviousValues};
use super::{ClientPaths, ClientSetupStatus};
use serde_json::Value;
use std::path::Path;
use toml_edit::{value, DocumentMut, Item, Table, TableLike, Value as TomlValue};

pub const PROVIDER_ID: &str = "yeapi";
pub const PROVIDER_NAME: &str = "YeAPI";
pub const WIRE_API: &str = "responses";

pub struct ProviderSpec<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub requires_openai_auth: bool,
}

pub struct DisconnectOutput {
    pub warnings: Vec<String>,
    pub remove_config_file: bool,
}

/// 首次接入前，记录我们准备接管的键的原始值。
pub fn capture(existing: &str) -> PreviousValues {
    let config_existed = !existing.trim().is_empty();
    let doc = match existing.parse::<DocumentMut>() {
        Ok(doc) => doc,
        Err(_) => {
            return PreviousValues {
                config_existed,
                ..Default::default()
            }
        }
    };

    let provider_toml = doc
        .get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(PROVIDER_ID))
        .map(|item| wrap_provider_item(PROVIDER_ID, item));

    PreviousValues {
        model_provider: doc
            .get("model_provider")
            .and_then(Item::as_str)
            .map(str::to_string),
        model: doc.get("model").and_then(Item::as_str).map(str::to_string),
        disable_response_storage: doc.get("disable_response_storage").and_then(Item::as_bool),
        provider_toml,
        model_providers_existed: doc.get("model_providers").is_some(),
        config_existed,
    }
}

/// 合并出新的 `config.toml` 文本。只改我们拥有的键，保留注释与其他段落。
pub fn merge_config(existing: &str, spec: &ProviderSpec<'_>) -> Result<String, String> {
    let mut doc: DocumentMut = if existing.trim().is_empty() {
        DocumentMut::new()
    } else {
        existing
            .parse()
            .map_err(|err| format!("config.toml 解析失败: {err}"))?
    };

    {
        let table = doc.as_table_mut();
        set_string(table, "model_provider", PROVIDER_ID);
        set_string(table, "model", spec.model);
        set_bool(table, "disable_response_storage", true);
    }

    let providers = ensure_model_providers(&mut doc)?;
    providers.remove(PROVIDER_ID);

    let mut provider = Table::new();
    provider.insert("name", value(PROVIDER_NAME));
    provider.insert("base_url", value(spec.base_url));
    provider.insert("wire_api", value(WIRE_API));
    provider.insert("experimental_bearer_token", value(spec.api_key));
    provider.insert("requires_openai_auth", value(spec.requires_openai_auth));
    providers.insert(PROVIDER_ID, Item::Table(provider));

    Ok(doc.to_string())
}

/// 判断磁盘上是否存在 ChatGPT 登录（`auth_mode=="chatgpt"` 且 access_token 非空）。
pub fn auth_logged_in_from_json(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    let chatgpt = value.get("auth_mode").and_then(Value::as_str) == Some("chatgpt");
    let token = value
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.trim().is_empty());
    chatgpt && token
}

pub fn auth_logged_in(auth_path: &Path) -> bool {
    std::fs::read_to_string(auth_path)
        .map(|text| auth_logged_in_from_json(&text))
        .unwrap_or(false)
}

pub fn status(
    paths: &ClientPaths,
    record: Option<&ClientRecord>,
    gateway_base_url: &str,
) -> ClientSetupStatus {
    let config_text = std::fs::read_to_string(&paths.config).ok();
    let (doc, config_parse_error) = parse_doc(config_text.as_deref());

    let current_model_provider = doc
        .as_ref()
        .and_then(|doc| doc.get("model_provider"))
        .and_then(Item::as_str)
        .map(str::to_string);
    let current_model = doc
        .as_ref()
        .and_then(|doc| doc.get("model"))
        .and_then(Item::as_str)
        .map(str::to_string);

    let token = doc.as_ref().and_then(provider_token);
    let yeapi_provider_present = doc.as_ref().is_some_and(provider_exists);
    let token_fingerprint = token.as_deref().map(state::fingerprint);

    let applied = record.is_some_and(|record| {
        current_model_provider.as_deref() == Some(PROVIDER_ID)
            && token_fingerprint.as_deref() == Some(record.key_fingerprint.as_str())
    });
    let drift = record.is_some() && !applied && config_parse_error.is_none();
    let auth_logged_in = auth_logged_in(&paths.auth);
    let provider_token_hint = token.as_deref().map(key_hint);

    ClientSetupStatus {
        client: super::ClientKind::Codex.as_str().to_string(),
        display_name: super::ClientKind::Codex.display_name().to_string(),
        installed: paths.home.exists(),
        applied,
        drift,
        config_parse_error,
        codex_home: paths.home.to_string_lossy().to_string(),
        config_path: paths.config.to_string_lossy().to_string(),
        auth_path: paths.auth.to_string_lossy().to_string(),
        auth_logged_in,
        requires_openai_auth: auth_logged_in,
        current_model_provider,
        current_model,
        yeapi_provider_present,
        provider_token_hint: provider_token_hint.clone(),
        applied_api_key_id: record.map(|record| record.api_key_id.clone()),
        applied_api_key_hint: if record.is_some() {
            provider_token_hint
        } else {
            None
        },
        applied_api_key_valid: false,
        applied_at: record.map(|record| record.applied_at.clone()),
        applied_base_url: record.map(|record| record.base_url.clone()),
        gateway_base_url: gateway_base_url.to_string(),
        base_url_matches: record
            .map(|record| record.base_url == gateway_base_url)
            .unwrap_or(false),
        backup_path: record.and_then(|record| record.backup_path.clone()),
    }
}

/// 断开接入：只恢复“当前值仍是 YeAPI 写入值”的字段。
pub fn disconnect_config(
    existing: &str,
    record: &ClientRecord,
) -> Result<(String, DisconnectOutput), String> {
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|err| format!("config.toml 解析失败，未做任何修改: {err}"))?;
    let mut warnings = Vec::new();

    let current_provider = doc
        .get("model_provider")
        .and_then(Item::as_str)
        .map(str::to_string);
    if current_provider.as_deref() == Some(record.provider_id.as_str()) {
        match &record.previous.model_provider {
            Some(previous) => set_string(doc.as_table_mut(), "model_provider", previous),
            None => {
                doc.remove("model_provider");
            }
        }
    } else if current_provider.is_some() {
        warnings.push("model_provider 已被其他工具修改，未恢复".to_string());
    }

    let current_model = doc.get("model").and_then(Item::as_str).map(str::to_string);
    if current_model.as_deref() == Some(record.model.as_str()) {
        match &record.previous.model {
            Some(previous) => set_string(doc.as_table_mut(), "model", previous),
            None => {
                doc.remove("model");
            }
        }
    } else if current_model.is_some() {
        warnings.push("model 已被其他工具修改，未恢复".to_string());
    }

    if doc.get("disable_response_storage").and_then(Item::as_bool) == Some(true) {
        match record.previous.disable_response_storage {
            Some(false) => set_bool(doc.as_table_mut(), "disable_response_storage", false),
            Some(true) => {}
            None => {
                doc.remove("disable_response_storage");
            }
        }
    }

    let providers_empty = {
        let providers = ensure_model_providers(&mut doc)?;
        let token_still_ours = providers
            .get(record.provider_id.as_str())
            .and_then(|provider| provider.get("experimental_bearer_token"))
            .and_then(Item::as_str)
            .map(|token| state::fingerprint(token) == record.key_fingerprint)
            .unwrap_or(false);

        if let Some(snippet) = &record.previous.provider_toml {
            match extract_provider_item(snippet, &record.provider_id) {
                Ok(item) => {
                    providers.insert(&record.provider_id, item);
                }
                Err(err) => {
                    warnings.push(format!("原 yeapi provider 恢复失败，已保留当前表：{err}"))
                }
            }
        } else if token_still_ours {
            providers.remove(&record.provider_id);
        } else if providers.get(record.provider_id.as_str()).is_some() {
            warnings.push("检测到 yeapi provider 已被外部修改，已保留".to_string());
        }

        providers.is_empty()
    };

    if providers_empty && !record.previous.model_providers_existed {
        doc.remove("model_providers");
    }

    let remove_config_file = !record.previous.config_existed && doc.as_table().is_empty();
    Ok((
        doc.to_string(),
        DisconnectOutput {
            warnings,
            remove_config_file,
        },
    ))
}

pub fn key_hint(token: &str) -> String {
    let count = token.chars().count();
    if count <= 4 {
        return token.to_string();
    }
    let tail: String = token.chars().skip(count - 4).collect();
    format!("****{tail}")
}

fn parse_doc(text: Option<&str>) -> (Option<DocumentMut>, Option<String>) {
    match text {
        None => (None, None),
        Some(text) if text.trim().is_empty() => (None, None),
        Some(text) => match text.parse::<DocumentMut>() {
            Ok(doc) => (Some(doc), None),
            Err(err) => (None, Some(format!("config.toml 解析失败: {err}"))),
        },
    }
}

fn provider_exists(doc: &DocumentMut) -> bool {
    doc.get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(PROVIDER_ID))
        .is_some()
}

fn provider_token(doc: &DocumentMut) -> Option<String> {
    doc.get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(PROVIDER_ID))
        .and_then(Item::as_table_like)
        .and_then(|provider| provider.get("experimental_bearer_token"))
        .and_then(Item::as_str)
        .map(str::to_string)
}

/// 取得（必要时创建）`model_providers` 表；内联表会被转成标准表。
fn ensure_model_providers<'a>(doc: &'a mut DocumentMut) -> Result<&'a mut dyn TableLike, String> {
    let item = doc
        .entry("model_providers")
        .or_insert(Item::Table(Table::new()));

    let inline_as_table = match &*item {
        Item::Table(_) => None,
        Item::Value(TomlValue::InlineTable(inline)) => Some(inline.clone().into_table()),
        _ => return Err("config.toml 中的 model_providers 不是表，无法安全合并".to_string()),
    };
    if let Some(table) = inline_as_table {
        *item = Item::Table(table);
    }

    item.as_table_like_mut()
        .ok_or_else(|| "config.toml 中的 model_providers 不是表，无法安全合并".to_string())
}

fn set_value(table: &mut dyn TableLike, key: &str, new_value: TomlValue) {
    if let Some(existing) = table.get_mut(key).and_then(Item::as_value_mut) {
        let decor = existing.decor().clone();
        let mut replaced = new_value;
        *replaced.decor_mut() = decor;
        *existing = replaced;
    } else {
        table.insert(key, Item::Value(new_value));
    }
}

fn set_string(table: &mut dyn TableLike, key: &str, new_value: &str) {
    set_value(table, key, TomlValue::from(new_value));
}

fn set_bool(table: &mut dyn TableLike, key: &str, new_value: bool) {
    set_value(table, key, TomlValue::from(new_value));
}

fn wrap_provider_item(id: &str, item: &Item) -> String {
    let mut doc = DocumentMut::new();
    let mut providers = Table::new();
    providers.set_implicit(true);
    providers.insert(id, item.clone());
    doc.insert("model_providers", Item::Table(providers));
    doc.to_string()
}

fn extract_provider_item(snippet: &str, id: &str) -> Result<Item, String> {
    let doc: DocumentMut = snippet
        .parse()
        .map_err(|err| format!("备份的 provider 片段无法解析: {err}"))?;
    doc.get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(id))
        .cloned()
        .ok_or_else(|| format!("备份片段中缺少 model_providers.{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>(key: &'a str) -> ProviderSpec<'a> {
        ProviderSpec {
            base_url: "http://127.0.0.1:8777/v1",
            model: "gpt-5",
            api_key: key,
            requires_openai_auth: false,
        }
    }

    fn record(previous: PreviousValues, model: &str, fingerprint: &str) -> ClientRecord {
        ClientRecord {
            applied_at: "now".to_string(),
            codex_home: "/tmp/.codex".to_string(),
            config_path: "/tmp/.codex/config.toml".to_string(),
            provider_id: PROVIDER_ID.to_string(),
            base_url: "http://127.0.0.1:8777/v1".to_string(),
            model: model.to_string(),
            api_key_id: "key-id".to_string(),
            key_fingerprint: fingerprint.to_string(),
            previous,
            backup_path: None,
        }
    }

    #[test]
    fn merge_preserves_comments_and_unrelated_sections() {
        let input = r#"# 我的配置
model_provider = "openai" # 原来的 provider
model = "gpt-4o"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"

[projects."/tmp/demo"]
trust_level = "trusted"
"#;
        let output = merge_config(input, &spec("sk-yeapi-test")).unwrap();
        assert!(output.contains("# 我的配置"));
        assert!(output.contains("model_provider = \"yeapi\" # 原来的 provider"));
        assert!(output.contains("[model_providers.openai]"));
        assert!(output.contains("[projects.\"/tmp/demo\"]"));
        assert!(output.contains("experimental_bearer_token = \"sk-yeapi-test\""));
        assert!(output.contains("disable_response_storage = true"));
    }

    #[test]
    fn merge_is_idempotent() {
        let once = merge_config("", &spec("k1")).unwrap();
        let twice = merge_config(&once, &spec("k1")).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_handles_inline_model_providers() {
        let input = "model_providers = { openai = { name = \"OpenAI\" } }\n";
        let output = merge_config(input, &spec("k")).unwrap();
        let doc: DocumentMut = output.parse().unwrap();
        assert!(doc
            .get("model_providers")
            .unwrap()
            .as_table_like()
            .is_some());
        assert_eq!(
            doc.get("model_providers")
                .and_then(Item::as_table_like)
                .and_then(|providers| providers.get("openai"))
                .is_some(),
            true
        );
    }

    #[test]
    fn auth_login_detection() {
        assert!(auth_logged_in_from_json(
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"tok"}}"#
        ));
        assert!(!auth_logged_in_from_json("{}"));
        assert!(!auth_logged_in_from_json(
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":""}}"#
        ));
        assert!(!auth_logged_in_from_json("not json"));
    }

    #[test]
    fn disconnect_restores_previous_values() {
        let original = "model_provider = \"openai\"\nmodel = \"gpt-4o\"\n";
        let applied = merge_config(original, &spec("k1")).unwrap();
        let fp = state::fingerprint("k1");
        let record = record(capture(original), "gpt-5", &fp);

        let (restored, output) = disconnect_config(&applied, &record).unwrap();
        assert!(restored.contains("model_provider = \"openai\""));
        assert!(restored.contains("model = \"gpt-4o\""));
        assert!(!restored.contains("[model_providers.yeapi]"));
        assert!(!output.remove_config_file);
        assert!(output.warnings.is_empty());
    }

    #[test]
    fn disconnect_skips_user_changed_values() {
        let applied = merge_config("", &spec("k1")).unwrap();
        let changed = applied.replace("model_provider = \"yeapi\"", "model_provider = \"other\"");
        let fp = state::fingerprint("k1");
        let record = record(capture(""), "gpt-5", &fp);

        let (restored, output) = disconnect_config(&changed, &record).unwrap();
        assert!(restored.contains("model_provider = \"other\""));
        assert!(!output.warnings.is_empty());
    }

    #[test]
    fn disconnect_restores_existing_yeapi_table() {
        let original = r#"model_provider = "yeapi"
model = "old"

[model_providers.yeapi]
name = "Old"
base_url = "https://old.example/v1"
wire_api = "responses"
experimental_bearer_token = "old-key"
"#;
        let applied = merge_config(original, &spec("new-key")).unwrap();
        let fp = state::fingerprint("new-key");
        let record = record(capture(original), "gpt-5", &fp);

        let (restored, _) = disconnect_config(&applied, &record).unwrap();
        assert!(restored.contains("name = \"Old\""));
        assert!(restored.contains("old-key"));
        assert!(!restored.contains("new-key"));
    }

    #[test]
    fn disconnect_removes_file_when_it_did_not_exist() {
        let applied = merge_config("", &spec("k1")).unwrap();
        let fp = state::fingerprint("k1");
        let record = record(capture(""), "gpt-5", &fp);
        let (restored, output) = disconnect_config(&applied, &record).unwrap();
        assert!(restored.trim().is_empty());
        assert!(output.remove_config_file);
    }

    #[test]
    fn status_detects_applied_and_drift() {
        let dir = std::env::temp_dir().join(format!(
            "yeapi-codex-status-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = ClientPaths {
            home: dir.clone(),
            config: dir.join("config.toml"),
            auth: dir.join("auth.json"),
        };
        let token = "sk-yeapi-testkey";
        let applied = merge_config("", &spec(token)).unwrap();
        std::fs::write(&paths.config, &applied).unwrap();
        let fp = state::fingerprint(token);
        let record = record(capture(""), "gpt-5", &fp);

        let applied_status = status(&paths, Some(&record), "http://127.0.0.1:8777/v1");
        assert!(applied_status.applied);
        assert!(!applied_status.drift);
        assert!(applied_status.yeapi_provider_present);
        assert_eq!(
            applied_status.provider_token_hint.as_deref(),
            Some("****tkey")
        );
        assert!(applied_status.base_url_matches);

        std::fs::write(&paths.config, applied.replace("yeapi", "other")).unwrap();
        let drifted_status = status(&paths, Some(&record), "http://127.0.0.1:8777/v1");
        assert!(!drifted_status.applied);
        assert!(drifted_status.drift);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
