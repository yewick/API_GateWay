use serde::{Deserialize, Serialize};

// ---------- 风险等级 ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Clean,    // 0 无风险
    Info,     // 1 提示（如外部 URL）
    Low,      // 2 低风险（邮箱、手机号）
    Medium,   // 3 中风险（本地路径、零宽字符）
    High,     // 4 高风险（密钥、敏感路径、IP 探测）
    Critical, // 5 严重（私钥、数据外传命令）
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Clean => "clean",
            RiskLevel::Info => "info",
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }

    pub fn rank(&self) -> u8 {
        match self {
            RiskLevel::Clean => 0,
            RiskLevel::Info => 1,
            RiskLevel::Low => 2,
            RiskLevel::Medium => 3,
            RiskLevel::High => 4,
            RiskLevel::Critical => 5,
        }
    }
}

impl Default for RiskLevel {
    fn default() -> Self {
        RiskLevel::Clean
    }
}

// ---------- 安全动作 ----------

/// 安全动作：扫描后决定怎么处理这个请求
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SecurityAction {
    Allow,   // 放行
    Warn,    // 警告（记录但放行）
    Redact,  // 脱敏后放行
    Confirm, // 需要确认（预留）
    Block,   // 阻断
}

impl Default for SecurityAction {
    fn default() -> Self {
        SecurityAction::Allow
    }
}

impl SecurityAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityAction::Allow => "allow",
            SecurityAction::Warn => "warn",
            SecurityAction::Redact => "redact",
            SecurityAction::Confirm => "confirm",
            SecurityAction::Block => "block",
        }
    }
}

// ---------- 单条风险发现 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub phase: String,           // request | response
    pub category: String,        // credential | file | unicode | network | tool | prompt
    pub rule_id: String,         // 如 credential.secret_token
    pub severity: RiskLevel,
    pub title: String,
    pub description: String,
    pub location: String,        // JSON 路径，如 $.messages[0].content
    pub evidence_masked: String, // 打码证据，如 sk-abc****wxyz
}

// ---------- 一次扫描的完整结果 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    pub risk_level: RiskLevel,
    pub risk_score: i32, // 0-100 综合评分
    pub action: SecurityAction,
    pub sanitized: bool,
    pub blocked_reason: Option<String>,
    pub summary: String,
    pub findings: Vec<SecurityFinding>,
}

impl Default for SecurityScanResult {
    fn default() -> Self {
        Self {
            risk_level: RiskLevel::Clean,
            risk_score: 0,
            action: SecurityAction::Allow,
            sanitized: false,
            blocked_reason: None,
            summary: String::new(),
            findings: Vec::new(),
        }
    }
}

// ---------- 安全设置 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub enabled: bool,            // 总开关
    pub mode: String,             // audit | warn | redact | confirm | block
    pub scan_request: bool,       // 扫描请求
    pub scan_response: bool,      // 扫描响应
    pub scan_unicode: bool,       // Unicode 隐写检测
    pub scan_tools: bool,         // 工具/命令风险检测
    pub scan_network: bool,       // 网络风险检测
    pub redact_secrets: bool,     // 强制脱敏
    pub block_on_critical: bool,  // Critical 强制阻断（无视 mode）
    pub max_scan_bytes: usize,    // 单字段最大扫描字节数（性能保护）
}

// ---------- 内置规则模型 ----------

/// 数据库行：security_builtin_rules
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BuiltinRule {
    pub id: String,
    pub rule_id: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub description: Option<String>,
    pub toggle_key: Option<String>,
    pub enabled: i32,
    pub created_at: String,
}

/// 内置规则更新入参
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuiltinRuleInput {
    pub enabled: Option<bool>,
    pub severity: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

// ---------- 自定义规则模型 ----------

/// 数据库行：security_custom_rules
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomRule {
    pub id: String,
    pub rule_type: String,
    pub category: String,
    pub pattern: String,
    pub severity: String,
    pub action: Option<String>,
    pub enabled: i32,
    pub description: Option<String>,
    pub created_at: String,
}

/// 自定义规则创建入参
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCustomRuleInput {
    pub rule_type: String,
    pub category: String,
    pub pattern: String,
    pub severity: Option<String>,
    pub action: Option<String>,
    pub description: Option<String>,
}
