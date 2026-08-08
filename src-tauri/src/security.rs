// ===== 安全审计引擎 =====
// 对入站请求做敏感信息扫描（凭证泄露、敏感路径等），
// 支持 audit / redact / block 三种策略。

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

// ---------- 数据类型 ----------

/// 安全动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityAction {
    Audit,
    Redact,
    Block,
}

impl SecurityAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityAction::Audit => "audit",
            SecurityAction::Redact => "redact",
            SecurityAction::Block => "block",
        }
    }
}

/// 风险等级（有序）
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::None => "none",
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }

    pub fn rank(&self) -> u8 {
        match self {
            RiskLevel::None => 0,
            RiskLevel::Low => 1,
            RiskLevel::Medium => 2,
            RiskLevel::High => 3,
            RiskLevel::Critical => 4,
        }
    }
}

/// 单条安全发现
#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: String,
    pub severity: String,
    pub detail: String,
}

/// 扫描结果
#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub action: SecurityAction,
    pub risk_level: RiskLevel,
    pub risk_score: u32,
    pub summary: String,
    pub blocked_reason: Option<String>,
    pub sanitized: bool,
    pub findings: Vec<Finding>,
}

impl Default for SecurityResult {
    fn default() -> Self {
        Self {
            action: SecurityAction::Audit,
            risk_level: RiskLevel::None,
            risk_score: 0,
            summary: String::new(),
            blocked_reason: None,
            sanitized: false,
            findings: Vec::new(),
        }
    }
}

/// 安全配置（从 Tauri Store 读取）
#[derive(Debug, Clone, Default)]
pub struct SecuritySettings {
    pub enabled: bool,
    pub mode: String, // "audit" | "redact" | "block"
    pub scan_unicode: bool,
    pub scan_tools: bool,
    pub scan_network: bool,
    pub scan_response: bool,
    pub redact_secrets: bool,
    pub block_on_critical: bool,
}

// ---------- 配置读取 ----------

pub fn get_security_settings(app: &AppHandle) -> SecuritySettings {
    let mut s = SecuritySettings::default();
    if let Ok(store) = app.store("settings.json") {
        s.enabled = store
            .get("security.enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        s.mode = store
            .get("security.mode")
            .and_then(|v| v.as_str().map(|m| m.to_string()))
            .unwrap_or_else(|| "audit".to_string());
        s.scan_unicode = store
            .get("security.scan_unicode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        s.scan_tools = store
            .get("security.scan_tools")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        s.scan_network = store
            .get("security.scan_network")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        s.scan_response = store
            .get("security.scan_response")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        s.redact_secrets = store
            .get("security.redact_secrets")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        s.block_on_critical = store
            .get("security.block_on_critical")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    }
    s
}

// ---------- 规则检测 ----------

/// 检测是否为疑似密钥字符串
fn is_secret_like(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with("sk-") && t.len() > 12)
        || t.starts_with("sk-ant-")
        || t.starts_with("sk-proj-")
        || (t.starts_with("AIza") && t.len() > 20)
        || t.starts_with("x-api-key:")
        || t.starts_with("authorization: bearer")
}

/// 检测是否为敏感路径 / 网络目标
fn is_sensitive_content(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("/etc/passwd")
        || lower.contains("/etc/shadow")
        || lower.contains(".ssh/")
        || lower.contains("~/.aws")
        || lower.contains("aws_secret_access_key")
        || lower.contains("private_key")
        || lower.contains("https://api.openai.com")
        || lower.contains("https://api.anthropic.com")
}

// ---------- 扫描逻辑 ----------

/// 递归收集请求体中所有字符串，返回 (findings, 是否含敏感内容)
fn collect_strings(value: &Value, out: &mut Vec<Finding>) {
    match value {
        Value::String(s) => {
            if is_secret_like(s) {
                out.push(Finding {
                    rule: "secret_key_leak".to_string(),
                    severity: "critical".to_string(),
                    detail: "检测到疑似 API 密钥".to_string(),
                });
            }
            if is_sensitive_content(s) {
                out.push(Finding {
                    rule: "sensitive_content".to_string(),
                    severity: "high".to_string(),
                    detail: "检测到敏感路径或内部资源引用".to_string(),
                });
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_strings(item, out);
            }
        }
        Value::Object(map) => {
            for (_k, v) in map {
                collect_strings(v, out);
            }
        }
        _ => {}
    }
}

fn worst_level(findings: &[Finding]) -> RiskLevel {
    let mut level = RiskLevel::None;
    for f in findings {
        let l = match f.severity.as_str() {
            "critical" => RiskLevel::Critical,
            "high" => RiskLevel::High,
            "medium" => RiskLevel::Medium,
            _ => RiskLevel::Low,
        };
        if l > level {
            level = l;
        }
    }
    level
}

/// 扫描请求体
pub fn scan_request(body: &Value, settings: &SecuritySettings) -> SecurityResult {
    let mut result = SecurityResult::default();
    if !settings.enabled {
        return result;
    }

    collect_strings(body, &mut result.findings);
    if result.findings.is_empty() {
        return result;
    }

    result.risk_level = worst_level(&result.findings);
    result.risk_score = (result.risk_level.rank() as u32) * 20;
    result.summary = format!(
        "检测到 {} 条风险规则命中（最高 {}）",
        result.findings.len(),
        result.risk_level.as_str()
    );

    // 策略判定
    if settings.mode == "block" {
        let threshold = if settings.block_on_critical {
            RiskLevel::Critical
        } else {
            RiskLevel::High
        };
        if result.risk_level >= threshold {
            result.action = SecurityAction::Block;
            result.blocked_reason = Some(result.summary.clone());
            return result;
        }
    }
    if settings.mode == "redact" || settings.redact_secrets {
        result.action = SecurityAction::Redact;
    }
    result
}

/// 脱敏请求体：将疑似密钥替换为 [REDACTED]
pub fn redact_request_body(body: &Value, _settings: &SecuritySettings) -> (Value, bool) {
    let mut changed = false;
    let new_value = redact_recursive(body, &mut changed);
    (new_value, changed)
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

/// 扫描响应体（可选开关）
pub fn scan_response(body: &Value, settings: &SecuritySettings) -> SecurityResult {
    let mut result = SecurityResult::default();
    if !settings.enabled || !settings.scan_response {
        return result;
    }
    collect_strings(body, &mut result.findings);
    result.risk_level = worst_level(&result.findings);
    result.risk_score = (result.risk_level.rank() as u32) * 20;
    if !result.findings.is_empty() {
        result.summary = format!(
            "响应侧检测到 {} 条风险规则命中",
            result.findings.len()
        );
    }
    result
}
