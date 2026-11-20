//! Claude 桌面端「第三方推理」档案（`<user-data-dir>/configLibrary`）的接入与恢复。
//!
//! 桌面端不读取 `~/.claude/settings.json` 的 `ANTHROPIC_*`，而是读取这里
//! `_meta.json` 指定的 gateway 档案。本模块新建一个 `YeAPI` 档案并切换过去，
//! 断开时恢复切换前的档案。

use super::state::{self, ClaudeDesktopRecord, ClientRecord};
use super::{ClaudeDesktopStatus, ClaudeModelAliases, DesktopPreview};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const PROFILE_NAME: &str = "YeAPI";
pub const CONFIG_LIBRARY_DIR: &str = "configLibrary";
pub const META_FILE: &str = "_meta.json";
pub const MASKED_KEY: &str = "<已隐藏，写入时使用所选密钥>";

/// 桌面端要求模型名看起来像 Anthropic 模型；这里用固定的路由名，
/// 由网关根据 `claude.desktop_aliases` 映射到真实模型。
pub const ROUTE_OPUS: &str = "claude-opus-5";
pub const ROUTE_SONNET: &str = "claude-sonnet-5";
pub const ROUTE_HAIKU: &str = "claude-haiku-4-5";
pub const ROUTE_FABLE: &str = "claude-fable-5";

pub struct DesktopSpec<'a> {
    /// 网关根地址（桌面端自己拼 `/v1/messages`）。
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub models: &'a ClaudeModelAliases,
}

pub struct ApplyInput<'a> {
    pub dir: &'a Path,
    pub spec: &'a DesktopSpec<'a>,
    pub app_data: &'a Path,
    pub existing: Option<&'a ClaudeDesktopRecord>,
}

pub struct ApplyOutcome {
    pub record: ClaudeDesktopRecord,
}

pub fn meta_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_LIBRARY_DIR).join(META_FILE)
}

pub fn profile_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(CONFIG_LIBRARY_DIR).join(format!("{id}.json"))
}

fn read_meta_value(dir: &Path) -> Result<Option<(String, Value)>, String> {
    let path = meta_path(dir);
    match std::fs::read_to_string(&path) {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) => {
            let value: Value = serde_json::from_str(&text)
                .map_err(|err| format!("读取桌面端推理档案失败（{}）: {err}", path.display()))?;
            if !value.is_object() {
                return Err(format!("桌面端推理档案 {} 顶层不是对象", path.display()));
            }
            Ok(Some((text, value)))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("读取 {} 失败: {err}", path.display())),
    }
}

fn entry_name(meta: &Value, id: &str) -> Option<String> {
    meta.get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        })
        .and_then(|entry| entry.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn entry_id_by_name(meta: &Value, name: &str) -> Option<String> {
    meta.get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name))
        })
        .and_then(|entry| entry.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// 构造桌面端的模型列表。`name` 必须是 Anthropic 风格的固定路由名（桌面端会
/// 校验模型名），`labelOverride` 展示 YeAPI 的实际模型。
pub fn build_models(models: &ClaudeModelAliases) -> Vec<Value> {
    vec![
        json!({
            "name": ROUTE_OPUS,
            "labelOverride": models.opus,
            "supports1m": true,
            "anthropicFamilyTier": "opus",
            "isFamilyDefault": true
        }),
        json!({
            "name": ROUTE_SONNET,
            "labelOverride": models.sonnet,
            "supports1m": true,
            "anthropicFamilyTier": "sonnet",
            "isFamilyDefault": true
        }),
        json!({
            "name": ROUTE_HAIKU,
            "labelOverride": models.haiku,
            "supports1m": true,
            "anthropicFamilyTier": "haiku",
            "isFamilyDefault": true
        }),
        json!({
            "name": ROUTE_FABLE,
            "labelOverride": models.fable,
            "supports1m": true,
            "anthropicFamilyTier": "fable",
            "isFamilyDefault": true
        }),
    ]
}

/// 桌面端路由名 → YeAPI 真实模型；由网关在 `/v1/messages` 入口解析。
pub fn alias_map(models: &ClaudeModelAliases) -> Value {
    let mut map = serde_json::Map::new();
    map.insert(ROUTE_OPUS.to_string(), json!(models.opus));
    map.insert(ROUTE_SONNET.to_string(), json!(models.sonnet));
    map.insert(ROUTE_HAIKU.to_string(), json!(models.haiku));
    map.insert(ROUTE_FABLE.to_string(), json!(models.fable));
    Value::Object(map)
}

/// 以现有档案为模板，只覆盖 YeAPI 接管的字段。
pub fn merge_profile(template: Option<&Value>, spec: &DesktopSpec<'_>) -> Value {
    let mut profile = match template {
        Some(value) if value.is_object() => value.clone(),
        _ => json!({}),
    };
    profile["inferenceProvider"] = json!("gateway");
    profile["inferenceGatewayBaseUrl"] = json!(spec.base_url);
    profile["inferenceGatewayAuthScheme"] = json!("bearer");
    profile["inferenceGatewayApiKey"] = json!(spec.api_key);
    profile["inferenceModels"] = Value::Array(build_models(spec.models));
    if profile.get("coworkEgressAllowedHosts").is_none() {
        profile["coworkEgressAllowedHosts"] = json!(["*"]);
    }
    if profile.get("disableDeploymentModeChooser").is_none() {
        profile["disableDeploymentModeChooser"] = json!(true);
    }
    profile
}

fn current_template(dir: &Path, meta: Option<&Value>) -> Option<Value> {
    let applied = meta
        .and_then(|value| value.get("appliedId"))
        .and_then(Value::as_str)?;
    let text = std::fs::read_to_string(profile_path(dir, applied)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn preview(dir: Option<&Path>, spec: &DesktopSpec<'_>) -> DesktopPreview {
    let masked = DesktopSpec {
        base_url: spec.base_url,
        api_key: MASKED_KEY,
        models: spec.models,
    };
    let (meta, template) = match dir {
        Some(dir) => {
            let meta = read_meta_value(dir).ok().flatten().map(|(_, value)| value);
            let template = current_template(dir, meta.as_ref());
            (meta, template)
        }
        None => (None, None),
    };
    let _ = meta;
    let profile = merge_profile(template.as_ref(), &masked);
    let snippet = serde_json::to_string_pretty(&profile)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .unwrap_or_else(|_| "{}".to_string());
    let config_path = dir
        .map(|dir| meta_path(dir).to_string_lossy().to_string())
        .unwrap_or_else(|| "未检测到 Claude 桌面端第三方推理配置".to_string());
    let warning = dir.is_none().then(|| {
        "未检测到 Claude 桌面端第三方推理配置，请先在桌面端开启 Developer Mode 并配置一次。"
            .to_string()
    });
    DesktopPreview {
        config_path,
        snippet,
        warning,
    }
}

pub fn status(
    dir: Option<&Path>,
    record: Option<&ClientRecord>,
    gateway_origin: &str,
) -> ClaudeDesktopStatus {
    let Some(dir) = dir else {
        return ClaudeDesktopStatus::default();
    };

    let meta_read = read_meta_value(dir).ok().flatten().map(|(_, value)| value);
    let applied_id = meta_read
        .as_ref()
        .and_then(|value| value.get("appliedId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let applied_profile = applied_id
        .as_ref()
        .and_then(|id| std::fs::read_to_string(profile_path(dir, id)).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());

    let applied_base_url = applied_profile
        .as_ref()
        .and_then(|value| value.get("inferenceGatewayBaseUrl"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let applied_models = applied_profile
        .as_ref()
        .and_then(|value| value.get("inferenceModels"))
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let our_id = record
        .and_then(|record| record.claude.as_ref())
        .and_then(|claude| claude.desktop.as_ref())
        .map(|desktop| desktop.profile_id.clone());
    let yeapi_present = match &our_id {
        Some(id) => meta_read
            .as_ref()
            .is_some_and(|meta| entry_name(meta, id).is_some()),
        None => meta_read
            .as_ref()
            .is_some_and(|meta| entry_id_by_name(meta, PROFILE_NAME).is_some()),
    };
    let applied = our_id
        .as_ref()
        .is_some_and(|id| applied_id.as_deref() == Some(id.as_str()))
        && applied_base_url.as_deref() == Some(gateway_origin);
    let drift = our_id.is_some() && !applied;

    ClaudeDesktopStatus {
        detected: true,
        config_dir: Some(dir.to_string_lossy().to_string()),
        meta_path: Some(meta_path(dir).to_string_lossy().to_string()),
        third_party_mode: true,
        applied_profile_name: applied_id
            .as_ref()
            .and_then(|id| meta_read.as_ref().and_then(|meta| entry_name(meta, id))),
        applied_base_url: applied_base_url.clone(),
        applied_models,
        yeapi_profile_present: yeapi_present,
        applied,
        drift,
        base_url_matches: applied_base_url.as_deref() == Some(gateway_origin),
    }
}

pub fn apply(input: ApplyInput<'_>) -> Result<ApplyOutcome, String> {
    let ApplyInput {
        dir,
        spec,
        app_data,
        existing,
    } = input;

    let meta_read = read_meta_value(dir)?;
    let (original_text, mut meta, previous_meta_existed) = match &meta_read {
        Some((text, value)) => (Some(text.clone()), value.clone(), true),
        None => (None, json!({ "entries": [] }), false),
    };
    let current_applied_id = meta
        .get("appliedId")
        .and_then(Value::as_str)
        .map(str::to_string);
    if meta
        .get("entries")
        .is_some_and(|entries| !entries.is_array())
    {
        return Err(format!(
            "桌面端配置索引 {} 中的 entries 不是数组，无法安全合并",
            meta_path(dir).display()
        ));
    }
    // 重复接入时保留首次接入前的 appliedId，断开时才能回到最初的档案。
    let previous_applied_id = existing
        .map(|desktop| desktop.previous_applied_id.clone())
        .unwrap_or(current_applied_id);

    // 选择复用（记录 / 同名）或新建档案。
    let record_id = existing.map(|desktop| desktop.profile_id.clone());
    let profile_id = record_id
        .filter(|id| entry_name(&meta, id).is_some())
        .or_else(|| entry_id_by_name(&meta, PROFILE_NAME))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let profile_file = profile_path(dir, &profile_id);
    let existing_profile_text = state::read_optional(&profile_file)?;
    let created_profile = match existing {
        Some(desktop) => desktop.created_profile,
        None => existing_profile_text.is_none(),
    };

    // 重复接入沿用首次接入前的备份，避免把「已切换为 YeAPI 的状态」当成原始状态。
    let previous_meta_backup = match existing.and_then(|d| d.previous_meta_backup.clone()) {
        Some(path) if Path::new(&path).exists() => Some(path),
        _ if previous_meta_existed => Some(
            state::backup_file(&meta_path(dir), app_data, "claude-desktop-meta")?
                .to_string_lossy()
                .to_string(),
        ),
        _ => None,
    };
    let previous_profile_backup = match existing.and_then(|d| d.previous_profile_backup.clone()) {
        Some(path) if Path::new(&path).exists() => Some(path),
        _ if !created_profile && profile_file.exists() => Some(
            state::backup_file(&profile_file, app_data, "claude-desktop-profile")?
                .to_string_lossy()
                .to_string(),
        ),
        _ => None,
    };
    let previous_meta_existed = existing
        .map(|desktop| desktop.previous_meta_existed)
        .unwrap_or(previous_meta_existed);

    // 模板优先取自己的旧档案，避免复用已存在的 YeAPI 档案时丢失字段。
    let template = existing_profile_text
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .or_else(|| current_template(dir, meta_read.as_ref().map(|(_, value)| value)));

    let profile = merge_profile(template.as_ref(), spec);
    let mut profile_text = serde_json::to_string_pretty(&profile)
        .map_err(|err| format!("序列化桌面端推理档案失败: {err}"))?;
    profile_text.push('\n');
    state::atomic_write_secret(&profile_file, profile_text.as_bytes())?;

    // 写入 meta 前确认没有被 CC Switch 等外部程序并发修改。
    let current_text = state::read_optional(&meta_path(dir))?;
    if current_text != original_text {
        restore_or_remove_profile(
            &profile_file,
            created_profile,
            previous_profile_backup.as_deref(),
        );
        return Err("桌面端推理档案在写入前被其他程序修改，请重试".to_string());
    }

    upsert_entry(&mut meta, &profile_id, PROFILE_NAME);
    meta["appliedId"] = json!(profile_id);
    let mut meta_text = serde_json::to_string_pretty(&meta)
        .map_err(|err| format!("序列化桌面端配置索引失败: {err}"))?;
    meta_text.push('\n');
    if let Err(err) = state::atomic_write(&meta_path(dir), meta_text.as_bytes(), 0o644) {
        restore_or_remove_profile(
            &profile_file,
            created_profile,
            previous_profile_backup.as_deref(),
        );
        return Err(err);
    }

    Ok(ApplyOutcome {
        record: ClaudeDesktopRecord {
            profile_id,
            profile_name: PROFILE_NAME.to_string(),
            config_dir: dir.to_string_lossy().to_string(),
            meta_path: meta_path(dir).to_string_lossy().to_string(),
            profile_path: profile_file.to_string_lossy().to_string(),
            previous_applied_id,
            previous_meta_existed,
            previous_meta_backup,
            previous_profile_backup,
            previous_aliases: None,
            created_profile,
        },
    })
}

fn upsert_entry(meta: &mut Value, id: &str, name: &str) {
    if !meta.get("entries").is_some_and(Value::is_array) {
        meta["entries"] = json!([]);
    }
    let entries = meta
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .expect("刚确保为数组");
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
    {
        entry["name"] = json!(name);
    } else {
        entries.push(json!({ "id": id, "name": name }));
    }
}

fn restore_or_remove_profile(profile_file: &Path, created: bool, backup: Option<&str>) {
    if created {
        let _ = std::fs::remove_file(profile_file);
    } else if let Some(backup) = backup {
        if let Ok(text) = std::fs::read_to_string(backup) {
            let _ = state::atomic_write_secret(profile_file, text.as_bytes());
        }
    }
}

/// 断开桌面端接入：仅当档案仍由我们应用时恢复。
pub fn disconnect(record: &ClaudeDesktopRecord, _app_data: &Path) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    let meta_file = PathBuf::from(&record.meta_path);
    let current_text = state::read_optional(&meta_file)?.unwrap_or_default();
    let current_applied = serde_json::from_str::<Value>(&current_text)
        .ok()
        .and_then(|value| {
            value
                .get("appliedId")
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    if current_applied.as_deref() != Some(record.profile_id.as_str()) {
        warnings.push("Claude 桌面端已切换到其他推理档案，未自动恢复".to_string());
        return Ok(warnings);
    }

    if let Some(backup) = &record.previous_meta_backup {
        let text = std::fs::read_to_string(backup)
            .map_err(|err| format!("读取桌面端配置备份失败: {err}"))?;
        state::atomic_write(&meta_file, text.as_bytes(), 0o644)?;
    } else {
        let mut meta: Value = serde_json::from_str(&current_text)
            .map_err(|err| format!("解析桌面端配置索引失败: {err}"))?;
        if let Some(entries) = meta.get_mut("entries").and_then(Value::as_array_mut) {
            entries.retain(|entry| {
                entry.get("id").and_then(Value::as_str) != Some(record.profile_id.as_str())
            });
        }
        match &record.previous_applied_id {
            Some(id) => meta["appliedId"] = json!(id),
            None => {
                if let Some(object) = meta.as_object_mut() {
                    object.remove("appliedId");
                }
            }
        }
        if record.previous_meta_existed {
            let mut text = serde_json::to_string_pretty(&meta)
                .map_err(|err| format!("序列化桌面端配置索引失败: {err}"))?;
            text.push('\n');
            state::atomic_write(&meta_file, text.as_bytes(), 0o644)?;
        } else {
            let _ = std::fs::remove_file(&meta_file);
        }
    }

    if record.created_profile {
        let _ = std::fs::remove_file(&record.profile_path);
    } else if let Some(backup) = &record.previous_profile_backup {
        let text = std::fs::read_to_string(backup)
            .map_err(|err| format!("读取桌面端档案备份失败: {err}"))?;
        state::atomic_write_secret(&PathBuf::from(&record.profile_path), text.as_bytes())?;
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_setup::state::ClaudeRecordData;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yeapi-claude-desktop-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(path.join(CONFIG_LIBRARY_DIR)).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn models() -> ClaudeModelAliases {
        ClaudeModelAliases {
            main: "deepseek-v4-pro".into(),
            opus: "deepseek-v4-pro".into(),
            sonnet: "deepseek-v4-flash".into(),
            haiku: "deepseek-v4-flash".into(),
            fable: "deepseek-v4-pro".into(),
        }
    }

    fn spec<'a>(key: &'a str, models: &'a ClaudeModelAliases) -> DesktopSpec<'a> {
        DesktopSpec {
            base_url: "http://127.0.0.1:8777",
            api_key: key,
            models,
        }
    }

    fn write_meta(dir: &Path, value: &Value) {
        std::fs::write(
            meta_path(dir),
            format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
        )
        .unwrap();
    }

    fn seed_ccswitch(dir: &Path) {
        write_meta(
            dir,
            &json!({
                "entries": [{ "id": "00000000-0000-4000-8000-000000157210", "name": "CC Switch" }],
                "appliedId": "00000000-0000-4000-8000-000000157210"
            }),
        );
        std::fs::write(
            profile_path(dir, "00000000-0000-4000-8000-000000157210"),
            serde_json::to_vec_pretty(&json!({
                "inferenceProvider": "gateway",
                "inferenceGatewayBaseUrl": "http://127.0.0.1:15721/claude-desktop",
                "inferenceGatewayAuthScheme": "bearer",
                "coworkEgressAllowedHosts": ["*"],
                "disableDeploymentModeChooser": true
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn merge_profile_overrides_gateway_fields_and_keeps_unknown() {
        let template = json!({
            "inferenceGatewayBaseUrl": "http://old",
            "coworkEgressAllowedHosts": ["example.com"],
            "customField": 1
        });
        let m = models();
        let profile = merge_profile(Some(&template), &spec("k", &m));
        assert_eq!(profile["inferenceProvider"], "gateway");
        assert_eq!(profile["inferenceGatewayBaseUrl"], "http://127.0.0.1:8777");
        assert_eq!(profile["inferenceGatewayApiKey"], "k");
        assert_eq!(profile["customField"], 1);
        assert_eq!(profile["coworkEgressAllowedHosts"][0], "example.com");
    }

    #[test]
    fn build_models_uses_anthropic_route_names() {
        let m = ClaudeModelAliases {
            main: "main-model".into(),
            opus: "p".into(),
            sonnet: "s".into(),
            haiku: "h".into(),
            fable: "f".into(),
        };
        let entries = build_models(&m);
        assert_eq!(entries.len(), 4);
        let names: Vec<&str> = entries
            .iter()
            .filter_map(|entry| entry.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(
            names,
            vec![ROUTE_OPUS, ROUTE_SONNET, ROUTE_HAIKU, ROUTE_FABLE]
        );
        let labels: Vec<&str> = entries
            .iter()
            .filter_map(|entry| entry.get("labelOverride").and_then(Value::as_str))
            .collect();
        assert_eq!(labels, vec!["p", "s", "h", "f"]);
        let tiers: Vec<&str> = entries
            .iter()
            .filter_map(|entry| entry.get("anthropicFamilyTier").and_then(Value::as_str))
            .collect();
        assert_eq!(tiers, vec!["opus", "sonnet", "haiku", "fable"]);
    }

    #[test]
    fn alias_map_covers_all_routes() {
        let m = models();
        let map = alias_map(&m);
        assert_eq!(map[ROUTE_OPUS], "deepseek-v4-pro");
        assert_eq!(map[ROUTE_SONNET], "deepseek-v4-flash");
        assert_eq!(map[ROUTE_HAIKU], "deepseek-v4-flash");
        assert_eq!(map[ROUTE_FABLE], "deepseek-v4-pro");
    }

    #[test]
    fn apply_creates_profile_and_switches_meta() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let outcome = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("sk-yeapi", &m),
            app_data: dir.path(),
            existing: None,
        })
        .unwrap();

        let meta: Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path(dir.path())).unwrap()).unwrap();
        assert_eq!(meta["appliedId"], outcome.record.profile_id);
        assert_eq!(meta["entries"].as_array().unwrap().len(), 2);
        assert!(outcome.record.previous_meta_backup.is_some());

        let profile: Value = serde_json::from_str(
            &std::fs::read_to_string(profile_path(dir.path(), &outcome.record.profile_id)).unwrap(),
        )
        .unwrap();
        assert_eq!(profile["inferenceGatewayBaseUrl"], "http://127.0.0.1:8777");
        assert_eq!(profile["inferenceGatewayApiKey"], "sk-yeapi");

        assert!(outcome.record.created_profile);
    }

    #[test]
    fn apply_reuses_existing_yeapi_entry() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let first = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("k1", &m),
            app_data: dir.path(),
            existing: None,
        })
        .unwrap();
        let second = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("k2", &m),
            app_data: dir.path(),
            existing: Some(&first.record),
        })
        .unwrap();

        assert_eq!(first.record.profile_id, second.record.profile_id);
        assert!(second.record.created_profile);
        let meta: Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path(dir.path())).unwrap()).unwrap();
        assert_eq!(meta["entries"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn reapply_keeps_first_snapshot_for_disconnect() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let first = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("k1", &m),
            app_data: dir.path(),
            existing: None,
        })
        .unwrap();
        let second = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("k2", &m),
            app_data: dir.path(),
            existing: Some(&first.record),
        })
        .unwrap();

        assert_eq!(
            second.record.previous_applied_id,
            first.record.previous_applied_id
        );
        assert_eq!(
            second.record.previous_meta_backup,
            first.record.previous_meta_backup
        );
        assert!(second.record.created_profile);

        let warnings = disconnect(&second.record, dir.path()).unwrap();
        assert!(warnings.is_empty());
        let meta: Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path(dir.path())).unwrap()).unwrap();
        assert_eq!(meta["appliedId"], "00000000-0000-4000-8000-000000157210");
        assert!(!profile_path(dir.path(), &second.record.profile_id).exists());
    }

    #[test]
    fn status_and_disconnect_restore_ccswitch() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let outcome = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("sk-yeapi", &m),
            app_data: dir.path(),
            existing: None,
        })
        .unwrap();

        let record = ClientRecord {
            applied_at: "now".into(),
            config_home: dir.path().to_string_lossy().to_string(),
            config_path: String::new(),
            base_url: "http://127.0.0.1:8777".into(),
            model: m.main.clone(),
            api_key_id: "id".into(),
            key_fingerprint: state::fingerprint("sk-yeapi"),
            provider_id: String::new(),
            previous: Default::default(),
            claude: Some(ClaudeRecordData {
                previous: Default::default(),
                applied_models: m.clone(),
                cli_applied: true,
                desktop: Some(outcome.record.clone()),
            }),
            backup_path: None,
            key_managed: false,
            key_name: None,
            key_allowed_models: Vec::new(),
            key_quota_limit: None,
            key_revoked: false,
            last_reconciled_at: None,
        };

        let status = self::status(Some(dir.path()), Some(&record), "http://127.0.0.1:8777");
        assert!(status.detected);
        assert!(status.applied);
        assert!(!status.drift);
        assert_eq!(status.applied_profile_name.as_deref(), Some(PROFILE_NAME));

        let warnings = disconnect(&outcome.record, dir.path()).unwrap();
        assert!(warnings.is_empty());
        let meta: Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path(dir.path())).unwrap()).unwrap();
        assert_eq!(meta["appliedId"], "00000000-0000-4000-8000-000000157210");
        assert_eq!(meta["entries"].as_array().unwrap().len(), 1);
        assert!(!profile_path(dir.path(), &outcome.record.profile_id).exists());
    }

    #[test]
    fn disconnect_skips_when_applied_profile_changed() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let outcome = apply(ApplyInput {
            dir: dir.path(),
            spec: &spec("k", &m),
            app_data: dir.path(),
            existing: None,
        })
        .unwrap();

        let mut meta: Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path(dir.path())).unwrap()).unwrap();
        meta["appliedId"] = json!("00000000-0000-4000-8000-000000157210");
        write_meta(dir.path(), &meta);

        let warnings = disconnect(&outcome.record, dir.path()).unwrap();
        assert!(!warnings.is_empty());
        assert!(profile_path(dir.path(), &outcome.record.profile_id).exists());
    }

    #[test]
    fn preview_masks_key() {
        let dir = TempDir::new();
        seed_ccswitch(dir.path());
        let m = models();
        let preview = self::preview(Some(dir.path()), &spec("sk-real", &m));
        assert!(!preview.snippet.contains("sk-real"));
        assert!(preview.snippet.contains(MASKED_KEY));
    }
}
