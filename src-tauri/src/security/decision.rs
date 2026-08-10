// ===== 策略决策器 + 入口函数 =====

use super::models::{RiskLevel, SecurityAction, SecurityScanResult, SecuritySettings};
use crate::security::scanner;
use serde_json::Value;

// ---------- 策略决策器 ----------

/// 根据扫描结果和安全设置，决定最终的安全动作
pub fn decide_action(result: &mut SecurityScanResult, settings: &SecuritySettings) {
    if !settings.enabled {
        result.action = SecurityAction::Allow;
        return;
    }

    result.action = match settings.mode.as_str() {
        // 只审计：全部放行，仅记录
        "off" | "audit" => SecurityAction::Allow,
        // 警告模式：Medium 以上标记 Warn（仍放行，前端可提示）
        "warn" => {
            if result.risk_level.rank() >= RiskLevel::Medium.rank() {
                SecurityAction::Warn
            } else {
                SecurityAction::Allow
            }
        }
        // 脱敏模式：High 以上先脱敏再转发
        "redact" => {
            if result.risk_level.rank() >= RiskLevel::High.rank() {
                SecurityAction::Redact
            } else {
                SecurityAction::Allow
            }
        }
        // 阻断模式：High 以上直接拒绝
        "block" => {
            if result.risk_level.rank() >= RiskLevel::High.rank() {
                SecurityAction::Block
            } else {
                SecurityAction::Allow
            }
        }
        _ => SecurityAction::Allow,
    };

    // 兜底：Critical 强制阻断开关（任何模式下都生效）
    if settings.block_on_critical && result.risk_level == RiskLevel::Critical {
        result.action = SecurityAction::Block;
    }

    if matches!(result.action, SecurityAction::Block) {
        result.blocked_reason = Some(result.summary.clone());
    }
}

// ---------- 入口函数 ----------

/// 扫描请求体（Proxy 层调用入口）
pub fn scan_request(body: &Value, settings: &SecuritySettings) -> SecurityScanResult {
    if !settings.enabled || !settings.scan_request {
        return SecurityScanResult::default();
    }
    let mut result = scanner::scan_json(body, "request", settings);
    decide_action(&mut result, settings);
    result
}

/// 扫描响应体（Proxy 层调用入口）
pub fn scan_response(body: &Value, settings: &SecuritySettings) -> SecurityScanResult {
    if !settings.enabled || !settings.scan_response {
        return SecurityScanResult::default();
    }
    let mut result = scanner::scan_json(body, "response", settings);
    decide_action(&mut result, settings);
    result
}

/// 脱敏请求体：将疑似密钥替换为 [REDACTED]
pub fn redact_request_body(body: &Value, _settings: &SecuritySettings) -> (Value, bool) {
    let mut changed = false;
    let new_value = redact_recursive(body, &mut changed);
    (new_value, changed)
}

fn is_secret_like(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with("sk-") && t.len() > 12)
        || t.starts_with("sk-ant-")
        || t.starts_with("sk-proj-")
        || (t.starts_with("AIza") && t.len() > 20)
        || t.starts_with("x-api-key:")
        || t.starts_with("authorization: bearer")
        || t.starts_with("ghp_")
        || t.starts_with("gho_")
        || t.starts_with("xoxb-")
        || t.starts_with("AKIA")
}

fn redact_recursive(value: &Value, changed: &mut bool) -> Value {
    match value {
        Value::String(s) => {
            if is_secret_like(s) {
                *changed = true;
                Value::String("[REDACTED]".to_string())
            } else {
                value.clone()
            }
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|item| redact_recursive(item, changed))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), redact_recursive(v, changed));
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}
