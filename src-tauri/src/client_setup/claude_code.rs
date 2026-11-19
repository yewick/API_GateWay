//! Claude Code 配置（`~/.claude/settings.json`）的检测、合并与断开。
//!
//! 只维护顶层 `env` 中与 YeAPI 接入相关的 9 个键，其余设置与键序保持不变。

use super::state::{self, ClaudePreviousValues, ClientRecord};
use super::{
    key_hint, ClaudeDesktopStatus, ClaudeEnvModels, ClaudeModelAliases, ClaudeStatusDetail,
    ClientPaths, ClientSetupStatus,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const AUTH_TOKEN_KEY: &str = "ANTHROPIC_AUTH_TOKEN";
pub const BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
pub const MODEL_KEY: &str = "ANTHROPIC_MODEL";
pub const OPUS_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL";
pub const SONNET_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL";
pub const HAIKU_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL";
pub const OPUS_NAME_KEY: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME";
pub const SONNET_NAME_KEY: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME";
pub const HAIKU_NAME_KEY: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME";

pub const MANAGED_ENV_KEYS: [&str; 9] = [
    AUTH_TOKEN_KEY,
    BASE_URL_KEY,
    MODEL_KEY,
    OPUS_MODEL_KEY,
    SONNET_MODEL_KEY,
    HAIKU_MODEL_KEY,
    OPUS_NAME_KEY,
    SONNET_NAME_KEY,
    HAIKU_NAME_KEY,
];

pub struct SettingsSpec<'a> {
    /// 网关根地址（Claude Code 自己拼 `/v1/messages`）。
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub models: &'a ClaudeModelAliases,
}

pub struct DisconnectOutput {
    pub warnings: Vec<String>,
    pub remove_settings_file: bool,
}

/// 接入时要写入的 `env` 键值。`_NAME` 键跟随对应模型 id，避免 ccswitch 旧显示名残留。
pub fn managed_values(spec: &SettingsSpec<'_>) -> [(&'static str, String); 9] {
    [
        (AUTH_TOKEN_KEY, spec.api_key.to_string()),
        (BASE_URL_KEY, spec.base_url.to_string()),
        (MODEL_KEY, spec.models.main.clone()),
        (OPUS_MODEL_KEY, spec.models.opus.clone()),
        (SONNET_MODEL_KEY, spec.models.sonnet.clone()),
        (HAIKU_MODEL_KEY, spec.models.haiku.clone()),
        (OPUS_NAME_KEY, spec.models.opus.clone()),
        (SONNET_NAME_KEY, spec.models.sonnet.clone()),
        (HAIKU_NAME_KEY, spec.models.haiku.clone()),
    ]
}

fn parse_root(existing: &str) -> Result<Value, String> {
    if existing.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value =
        serde_json::from_str(existing).map_err(|err| format!("settings.json 解析失败: {err}"))?;
    if !value.is_object() {
        return Err("settings.json 顶层不是 JSON 对象，无法安全合并".to_string());
    }
    Ok(value)
}

/// 合并出新的 `settings.json` 文本。只改 `env` 下受管的 9 个键。
pub fn merge_settings(existing: &str, spec: &SettingsSpec<'_>) -> Result<String, String> {
    let mut root = parse_root(existing)?;

    {
        let obj = root.as_object_mut().expect("parse_root 已校验为对象");
        let env = match obj.get_mut("env") {
            Some(Value::Object(map)) => map,
            Some(_) => return Err("settings.json 中的 env 不是对象，无法安全合并".to_string()),
            None => {
                obj.insert("env".to_string(), Value::Object(Map::new()));
                obj.get_mut("env")
                    .and_then(Value::as_object_mut)
                    .expect("刚插入的 env")
            }
        };
        for (key, value) in managed_values(spec) {
            env.insert(key.to_string(), Value::String(value));
        }
    }

    let mut text = serde_json::to_string_pretty(&root)
        .map_err(|err| format!("序列化 settings.json 失败: {err}"))?;
    text.push('\n');
    Ok(text)
}

/// 首次接入前，记录受管 env 键的原始值。
pub fn capture(existing: &str) -> ClaudePreviousValues {
    let settings_existed = !existing.trim().is_empty();
    let Ok(value) = serde_json::from_str::<Value>(existing) else {
        return ClaudePreviousValues {
            settings_existed,
            ..Default::default()
        };
    };
    let Some(env) = value.get("env").and_then(Value::as_object) else {
        return ClaudePreviousValues {
            settings_existed,
            ..Default::default()
        };
    };

    let mut values = BTreeMap::new();
    for key in MANAGED_ENV_KEYS {
        if let Some(value) = env.get(key) {
            values.insert(key.to_string(), value.clone());
        }
    }
    ClaudePreviousValues {
        env: values,
        env_existed: true,
        settings_existed,
    }
}

pub fn status(
    paths: &ClientPaths,
    record: Option<&ClientRecord>,
    gateway_origin: &str,
    desktop: ClaudeDesktopStatus,
) -> ClientSetupStatus {
    let settings_text = std::fs::read_to_string(&paths.config).ok();
    let (root, config_parse_error) = match settings_text.as_deref() {
        None => (None, None),
        Some(text) if text.trim().is_empty() => (None, None),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(value) if value.is_object() => (Some(value), None),
            Ok(_) => (None, Some("settings.json 顶层不是 JSON 对象".to_string())),
            Err(err) => (None, Some(format!("settings.json 解析失败: {err}"))),
        },
    };

    let env = root
        .as_ref()
        .and_then(|value| value.get("env"))
        .and_then(Value::as_object);
    let env_string = |key: &str| {
        env.and_then(|map| map.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let current_model = env_string(MODEL_KEY);
    let current_base_url = env_string(BASE_URL_KEY);
    let token = env_string(AUTH_TOKEN_KEY);
    let token_fingerprint = token.as_deref().map(state::fingerprint);

    let applied = record.is_some_and(|record| {
        token_fingerprint.as_deref() == Some(record.key_fingerprint.as_str())
            && current_base_url.as_deref() == Some(record.base_url.as_str())
    });
    let drift = record.is_some() && !applied && config_parse_error.is_none();
    let token_hint = token.as_deref().map(key_hint);

    ClientSetupStatus {
        client: super::ClientKind::Claude.as_str().to_string(),
        display_name: super::ClientKind::Claude.display_name().to_string(),
        installed: paths.home.exists(),
        applied,
        drift,
        config_parse_error,
        config_home: paths.home.to_string_lossy().to_string(),
        config_path: paths.config.to_string_lossy().to_string(),
        current_model,
        applied_api_key_id: record.map(|record| record.api_key_id.clone()),
        applied_api_key_hint: if record.is_some() { token_hint } else { None },
        applied_api_key_valid: false,
        applied_at: record.map(|record| record.applied_at.clone()),
        applied_base_url: record.map(|record| record.base_url.clone()),
        gateway_base_url: gateway_origin.to_string(),
        base_url_matches: record
            .map(|record| record.base_url == gateway_origin)
            .unwrap_or(false),
        backup_path: record.and_then(|record| record.backup_path.clone()),
        codex: None,
        claude: Some(ClaudeStatusDetail {
            env_present: env.is_some(),
            current_base_url,
            env_models: ClaudeEnvModels {
                opus: env_string(OPUS_MODEL_KEY),
                sonnet: env_string(SONNET_MODEL_KEY),
                haiku: env_string(HAIKU_MODEL_KEY),
            },
            top_level_model: root
                .as_ref()
                .and_then(|value| value.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string),
            anthropic_api_key_present: env
                .map(|map| map.contains_key("ANTHROPIC_API_KEY"))
                .unwrap_or(false),
            desktop,
        }),
    }
}

/// 断开接入：只恢复“当前值仍是 YeAPI 写入值”的键。
pub fn disconnect_settings(
    existing: &str,
    record: &ClientRecord,
) -> Result<(String, DisconnectOutput), String> {
    let claude = record
        .claude
        .as_ref()
        .ok_or_else(|| "接入记录缺少 Claude 配置".to_string())?;
    let mut root = parse_root(existing)?;
    let mut warnings = Vec::new();

    {
        let obj = root.as_object_mut().expect("parse_root 已校验为对象");
        if let Some(env) = obj.get_mut("env").and_then(Value::as_object_mut) {
            // token 只存指纹，单独比较。
            if let Some(value) = env.get(AUTH_TOKEN_KEY).and_then(Value::as_str) {
                if state::fingerprint(value) == record.key_fingerprint {
                    restore_or_remove(env, AUTH_TOKEN_KEY, &claude.previous);
                } else {
                    warnings.push(format!("{AUTH_TOKEN_KEY} 已被其他工具或用户修改，未恢复"));
                }
            }

            let applied = &claude.applied_models;
            let our_values: [(&str, &str); 8] = [
                (BASE_URL_KEY, record.base_url.as_str()),
                (MODEL_KEY, applied.main.as_str()),
                (OPUS_MODEL_KEY, applied.opus.as_str()),
                (SONNET_MODEL_KEY, applied.sonnet.as_str()),
                (HAIKU_MODEL_KEY, applied.haiku.as_str()),
                (OPUS_NAME_KEY, applied.opus.as_str()),
                (SONNET_NAME_KEY, applied.sonnet.as_str()),
                (HAIKU_NAME_KEY, applied.haiku.as_str()),
            ];
            for (key, ours) in our_values {
                let Some(current) = env.get(key) else {
                    continue;
                };
                if current.as_str() == Some(ours) {
                    restore_or_remove(env, key, &claude.previous);
                } else {
                    warnings.push(format!("{key} 已被其他工具或用户修改，未恢复"));
                }
            }

            if env.is_empty() && !claude.previous.env_existed {
                obj.remove("env");
            }
        }
    }

    let remove_settings_file =
        !claude.previous.settings_existed && root.as_object().is_some_and(|obj| obj.is_empty());
    let mut text = serde_json::to_string_pretty(&root)
        .map_err(|err| format!("序列化 settings.json 失败: {err}"))?;
    text.push('\n');
    Ok((
        text,
        DisconnectOutput {
            warnings,
            remove_settings_file,
        },
    ))
}

fn restore_or_remove(env: &mut Map<String, Value>, key: &str, previous: &ClaudePreviousValues) {
    match previous.env.get(key) {
        Some(value) => {
            env.insert(key.to_string(), value.clone());
        }
        None => {
            env.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_setup::state::ClaudeRecordData;

    fn models() -> ClaudeModelAliases {
        ClaudeModelAliases {
            main: "deepseek-v4-pro".into(),
            opus: "deepseek-v4-pro".into(),
            sonnet: "deepseek-v4-flash".into(),
            haiku: "deepseek-v4-flash".into(),
        }
    }

    fn spec<'a>(key: &'a str, models: &'a ClaudeModelAliases) -> SettingsSpec<'a> {
        SettingsSpec {
            base_url: "http://127.0.0.1:8777",
            api_key: key,
            models,
        }
    }

    fn record(
        previous: ClaudePreviousValues,
        models: ClaudeModelAliases,
        fingerprint: &str,
    ) -> ClientRecord {
        ClientRecord {
            applied_at: "now".into(),
            config_home: "/tmp/.claude".into(),
            config_path: "/tmp/.claude/settings.json".into(),
            base_url: "http://127.0.0.1:8777".into(),
            model: models.main.clone(),
            api_key_id: "key-id".into(),
            key_fingerprint: fingerprint.into(),
            provider_id: String::new(),
            previous: Default::default(),
            claude: Some(ClaudeRecordData {
                previous,
                applied_models: models,
                cli_applied: true,
                desktop: None,
            }),
            backup_path: None,
        }
    }

    #[test]
    fn merge_preserves_unknown_fields_and_order() {
        let input = r#"{
  "effortLevel": "high",
  "enabledPlugins": { "rust-analyzer-lsp@claude-plugins-official": true },
  "env": { "KEEP_ME": "1" },
  "modelSettings": { "claude-sonnet-5": { "effortLevel": "high" } },
  "skipWorkflowUsageWarning": true,
  "theme": "auto"
}
"#;
        let output = merge_settings(input, &spec("sk-yeapi-test", &models())).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["effortLevel"], "high");
        assert!(value["enabledPlugins"].is_object());
        assert!(value["modelSettings"].is_object());
        assert_eq!(value["skipWorkflowUsageWarning"], true);
        assert_eq!(value["theme"], "auto");
        assert_eq!(value["env"]["KEEP_ME"], "1");
        assert_eq!(value["env"][AUTH_TOKEN_KEY], "sk-yeapi-test");
        assert_eq!(value["env"][BASE_URL_KEY], "http://127.0.0.1:8777");
        assert_eq!(value["env"][MODEL_KEY], "deepseek-v4-pro");
        assert_eq!(value["env"][OPUS_NAME_KEY], "deepseek-v4-pro");
        assert_eq!(value["env"][SONNET_NAME_KEY], "deepseek-v4-flash");

        // 顶层键顺序保持不变（preserve_order）。
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "effortLevel",
                "enabledPlugins",
                "env",
                "modelSettings",
                "skipWorkflowUsageWarning",
                "theme"
            ]
        );
    }

    #[test]
    fn merge_is_idempotent() {
        let once = merge_settings("", &spec("k1", &models())).unwrap();
        let twice = merge_settings(&once, &spec("k1", &models())).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_rejects_invalid_json_and_non_object_shapes() {
        assert!(merge_settings("not json", &spec("k", &models())).is_err());
        assert!(merge_settings("[1,2]", &spec("k", &models())).is_err());
        assert!(merge_settings(r#"{"env": "oops"}"#, &spec("k", &models())).is_err());
    }

    #[test]
    fn capture_records_only_managed_keys() {
        let previous = capture(r#"{"theme":"dark","env":{"ANTHROPIC_MODEL":"old","KEEP":"x"}}"#);
        assert!(previous.settings_existed);
        assert!(previous.env_existed);
        assert_eq!(
            previous.env.get(MODEL_KEY).and_then(Value::as_str),
            Some("old")
        );
        assert!(!previous.env.contains_key("KEEP"));
        assert_eq!(previous.env.len(), 1);
    }

    #[test]
    fn disconnect_restores_previous_values() {
        let original = r#"{"theme":"auto","env":{"ANTHROPIC_MODEL":"old-model","ANTHROPIC_BASE_URL":"https://old.example"}}"#;
        let applied = merge_settings(original, &spec("k1", &models())).unwrap();
        let fp = state::fingerprint("k1");
        let record = record(capture(original), models(), &fp);

        let (restored, output) = disconnect_settings(&applied, &record).unwrap();
        let value: Value = serde_json::from_str(&restored).unwrap();
        assert_eq!(value["theme"], "auto");
        assert_eq!(value["env"][MODEL_KEY], "old-model");
        assert_eq!(value["env"][BASE_URL_KEY], "https://old.example");
        assert!(value["env"].get(AUTH_TOKEN_KEY).is_none());
        assert!(value["env"].get(OPUS_MODEL_KEY).is_none());
        assert!(output.warnings.is_empty());
        assert!(!output.remove_settings_file);
    }

    #[test]
    fn disconnect_skips_user_changed_values() {
        let applied = merge_settings("", &spec("k1", &models())).unwrap();
        let changed = applied.replace("deepseek-v4-pro", "user-picked");
        let fp = state::fingerprint("k1");
        let record = record(capture(""), models(), &fp);

        let (restored, output) = disconnect_settings(&changed, &record).unwrap();
        assert!(restored.contains("user-picked"));
        assert!(!output.warnings.is_empty());
    }

    #[test]
    fn disconnect_removes_env_created_by_us() {
        let applied = merge_settings("", &spec("k1", &models())).unwrap();
        let fp = state::fingerprint("k1");
        let record = record(capture(""), models(), &fp);

        let (restored, output) = disconnect_settings(&applied, &record).unwrap();
        let value: Value = serde_json::from_str(&restored).unwrap();
        assert!(value.get("env").is_none());
        assert!(output.remove_settings_file);
    }

    #[test]
    fn disconnect_keeps_other_settings() {
        let original = r#"{"theme":"auto"}"#;
        let applied = merge_settings(original, &spec("k1", &models())).unwrap();
        let fp = state::fingerprint("k1");
        let record = record(capture(original), models(), &fp);

        let (restored, output) = disconnect_settings(&applied, &record).unwrap();
        let value: Value = serde_json::from_str(&restored).unwrap();
        assert_eq!(value["theme"], "auto");
        assert!(value.get("env").is_none());
        assert!(!output.remove_settings_file);
    }

    #[test]
    fn status_detects_applied_and_drift() {
        let dir = std::env::temp_dir().join(format!(
            "yeapi-claude-status-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = ClientPaths {
            home: dir.clone(),
            config: dir.join("settings.json"),
            auth: None,
        };
        let token = "sk-yeapi-testkey";
        let applied = merge_settings("", &spec(token, &models())).unwrap();
        std::fs::write(&paths.config, &applied).unwrap();
        let fp = state::fingerprint(token);
        let record = record(capture(""), models(), &fp);

        let status = super::status(
            &paths,
            Some(&record),
            "http://127.0.0.1:8777",
            ClaudeDesktopStatus::default(),
        );
        assert!(status.applied);
        assert!(!status.drift);
        assert_eq!(status.applied_api_key_hint.as_deref(), Some("****tkey"));

        std::fs::write(&paths.config, applied.replace(token, "sk-yeapi-other")).unwrap();
        let drifted = super::status(
            &paths,
            Some(&record),
            "http://127.0.0.1:8777",
            ClaudeDesktopStatus::default(),
        );
        assert!(!drifted.applied);
        assert!(drifted.drift);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_contains_only_env() {
        let snippet = merge_settings("", &spec("<hidden>", &json_models())).unwrap();
        let value: Value = serde_json::from_str(&snippet).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert!(value["env"].is_object());
    }

    fn json_models() -> ClaudeModelAliases {
        models()
    }
}
