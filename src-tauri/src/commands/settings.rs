use serde::{Deserialize, Serialize};

use crate::services::knowledge::rag::DEFAULT_EMBEDDING_MODEL;

// ---------- 设置读写（基于 Tauri Store） ----------

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub server_port: u16,
    pub server_host: String,
    pub ui_theme: String,
    pub ui_language: String,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub auto_start: bool,
    pub retry_enabled: bool,
    pub retry_times: i32,
    pub default_embedding_model: String,
    /// MinerU token（空串 → Agent 轻量 API；非空 → Precise API）
    pub mineru_token: String,
    pub mineru_base_url: String,
    pub mineru_model: String,
    pub security_enabled: bool,
    pub security_mode: String,
    pub security_scan_request: bool,
    pub security_scan_unicode: bool,
    pub security_scan_tools: bool,
    pub security_scan_network: bool,
    pub security_scan_response: bool,
    pub security_redact_secrets: bool,
    pub security_block_on_critical: bool,
}

#[tauri::command]
pub async fn get_settings(
    app: tauri::AppHandle,
) -> Result<SettingsResponse, String> {
    use tauri_plugin_store::StoreExt;

    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    let get_bool = |key: &str, def: bool| -> bool {
        store.get(key).and_then(|v| v.as_bool()).unwrap_or(def)
    };
    let get_str = |key: &str, def: &str| -> String {
        store
            .get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| def.to_string())
    };
    let get_u16 = |key: &str, def: u16| -> u16 {
        store
            .get(key)
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(def)
    };
    let get_i32 = |key: &str, def: i32| -> i32 {
        store
            .get(key)
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(def)
    };
    // MinerU 三项：store 优先，环境变量次之（与 MinerUConfig::resolve 的优先级一致）
    let get_str_env = |key: &str, env_name: &str, def: &str| -> String {
        store
            .get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var(env_name)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| def.to_string())
    };

    Ok(SettingsResponse {
        server_port: get_u16("server.port", 8777),
        server_host: get_str("server.host", "127.0.0.1"),
        ui_theme: get_str("ui.theme", "dark"),
        ui_language: get_str("ui.language", "zh-CN"),
        minimize_to_tray: get_bool("ui.minimize_to_tray", true),
        close_to_tray: get_bool("ui.close_to_tray", false),
        auto_start: get_bool("ui.auto_start", false),
        retry_enabled: get_bool("retry.enabled", true),
        retry_times: get_i32("retry.times", 3),
        default_embedding_model: get_str("knowledge.default_embedding_model", DEFAULT_EMBEDDING_MODEL),
        mineru_token: get_str_env("knowledge.mineru.token", "YEAPI_MINERU_TOKEN", ""),
        mineru_base_url: get_str_env(
            "knowledge.mineru.base_url",
            "YEAPI_MINERU_BASE_URL",
            "https://mineru.net",
        ),
        mineru_model: get_str_env("knowledge.mineru.model", "YEAPI_MINERU_MODEL", "pipeline"),
        security_enabled: get_bool("security.enabled", true),
        security_mode: get_str("security.mode", "audit"),
        security_scan_request: get_bool("security.scan_request", true),
        security_scan_unicode: get_bool("security.scan_unicode", false),
        security_scan_tools: get_bool("security.scan_tools", true),
        security_scan_network: get_bool("security.scan_network", true),
        security_scan_response: get_bool("security.scan_response", false),
        security_redact_secrets: get_bool("security.redact_secrets", true),
        security_block_on_critical: get_bool("security.block_on_critical", false),
    })
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingsInput {
    pub server_port: Option<u16>,
    pub server_host: Option<String>,
    pub ui_theme: Option<String>,
    pub ui_language: Option<String>,
    pub minimize_to_tray: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub auto_start: Option<bool>,
    pub retry_enabled: Option<bool>,
    pub retry_times: Option<i32>,
    pub default_embedding_model: Option<String>,
    pub mineru_token: Option<String>,
    pub mineru_base_url: Option<String>,
    pub mineru_model: Option<String>,
    pub security_enabled: Option<bool>,
    pub security_mode: Option<String>,
    pub security_scan_request: Option<bool>,
    pub security_scan_unicode: Option<bool>,
    pub security_scan_tools: Option<bool>,
    pub security_scan_network: Option<bool>,
    pub security_scan_response: Option<bool>,
    pub security_redact_secrets: Option<bool>,
    pub security_block_on_critical: Option<bool>,
}

#[tauri::command]
pub async fn save_settings(
    app: tauri::AppHandle,
    settings: SaveSettingsInput,
) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;

    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    if let Some(v) = settings.server_port {
        store.set("server.port", serde_json::Value::Number((v as i32).into()));
    }
    if let Some(ref v) = settings.server_host {
        store.set("server.host", serde_json::Value::String(v.clone()));
    }
    if let Some(ref v) = settings.ui_theme {
        store.set("ui.theme", serde_json::Value::String(v.clone()));
    }
    if let Some(ref v) = settings.ui_language {
        store.set("ui.language", serde_json::Value::String(v.clone()));
    }
    if let Some(v) = settings.minimize_to_tray {
        store.set("ui.minimize_to_tray", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.close_to_tray {
        store.set("ui.close_to_tray", serde_json::Value::Bool(v));
    }
    let mut auto_start_changed: Option<bool> = None;
    if let Some(v) = settings.auto_start {
        let current = store
            .get("ui.auto_start")
            .and_then(|val| val.as_bool())
            .unwrap_or(false);
        if current != v {
            auto_start_changed = Some(v);
        }
        store.set("ui.auto_start", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.retry_enabled {
        store.set("retry.enabled", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.retry_times {
        store.set("retry.times", serde_json::Value::Number((v as i32).into()));
    }
    if let Some(ref v) = settings.default_embedding_model {
        store.set("knowledge.default_embedding_model", serde_json::Value::String(v.clone()));
    }
    if let Some(ref v) = settings.mineru_token {
        store.set("knowledge.mineru.token", serde_json::Value::String(v.clone()));
    }
    if let Some(ref v) = settings.mineru_base_url {
        store.set("knowledge.mineru.base_url", serde_json::Value::String(v.clone()));
    }
    if let Some(ref v) = settings.mineru_model {
        store.set("knowledge.mineru.model", serde_json::Value::String(v.clone()));
    }
    if let Some(v) = settings.security_enabled {
        store.set("security.enabled", serde_json::Value::Bool(v));
    }
    if let Some(ref v) = settings.security_mode {
        store.set("security.mode", serde_json::Value::String(v.clone()));
    }
    if let Some(v) = settings.security_scan_request {
        store.set("security.scan_request", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_scan_unicode {
        store.set("security.scan_unicode", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_scan_tools {
        store.set("security.scan_tools", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_scan_network {
        store.set("security.scan_network", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_scan_response {
        store.set("security.scan_response", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_redact_secrets {
        store.set("security.redact_secrets", serde_json::Value::Bool(v));
    }
    if let Some(v) = settings.security_block_on_critical {
        store.set("security.block_on_critical", serde_json::Value::Bool(v));
    }
    store.save().map_err(|e| e.to_string())?;

    // 开机自启：仅当值变化时应用到操作系统（LaunchAgent / 注册表 / autostart.desktop）
    if let Some(enabled) = auto_start_changed {
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        if enabled {
            autostart.enable().map_err(|e| e.to_string())?;
        } else {
            autostart.disable().map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
