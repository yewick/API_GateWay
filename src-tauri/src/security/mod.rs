pub mod decision;
pub mod models;
pub mod redact;
pub mod rules;
pub mod scanner;

// 重导出常用类型，兼容旧代码路径
pub use decision::{decide_action, redact_request_body, scan_request, scan_response};
pub use models::{
    BuiltinRule, CreateCustomRuleInput, CustomRule, RiskLevel, SecurityAction, SecurityFinding,
    SecurityScanResult, SecuritySettings, UpdateBuiltinRuleInput,
};
pub use redact::redact_json;
pub use rules::{
    apply_custom_rules, is_whitelisted, seed_builtin_rules, BuiltinRuleRepository,
    CustomRuleRepository,
};
pub use scanner::scan_json;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// 从 Tauri Store 读取安全配置
pub fn get_security_settings(app: &AppHandle) -> SecuritySettings {
    let mut s = SecuritySettings {
        enabled: true,
        mode: "audit".to_string(),
        scan_request: true,
        scan_response: false,
        scan_unicode: false,
        scan_tools: true,
        scan_network: true,
        redact_secrets: true,
        block_on_critical: false,
        max_scan_bytes: 65536,
    };

    if let Ok(store) = app.store("settings.json") {
        s.enabled = store
            .get("security.enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.enabled);
        s.mode = store
            .get("security.mode")
            .and_then(|v| v.as_str().map(|m| m.to_string()))
            .unwrap_or(s.mode);
        s.scan_request = store
            .get("security.scan_request")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.scan_request);
        s.scan_response = store
            .get("security.scan_response")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.scan_response);
        s.scan_unicode = store
            .get("security.scan_unicode")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.scan_unicode);
        s.scan_tools = store
            .get("security.scan_tools")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.scan_tools);
        s.scan_network = store
            .get("security.scan_network")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.scan_network);
        s.redact_secrets = store
            .get("security.redact_secrets")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.redact_secrets);
        s.block_on_critical = store
            .get("security.block_on_critical")
            .and_then(|v| v.as_bool())
            .unwrap_or(s.block_on_critical);
        s.max_scan_bytes = store
            .get("security.max_scan_bytes")
            .and_then(|v| v.as_i64())
            .unwrap_or(s.max_scan_bytes as i64) as usize;
    }
    s
}
