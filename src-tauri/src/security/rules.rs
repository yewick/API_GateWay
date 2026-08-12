// ===== 安全规则管理 =====
// 内置规则播种、CRUD，自定义规则 CRUD 与应用。

use sqlx::SqlitePool;

use super::models::{
    BuiltinRule, CreateCustomRuleInput, CustomRule, SecurityFinding, UpdateBuiltinRuleInput,
};
use super::scanner;

// ---------- 内置规则播种 ----------

pub async fn seed_builtin_rules(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let seeds = [
        // ── 凭证类（5条）──
        ("b001", "credential.secret_token", "credential", "high",
         "疑似密钥/Token", "检测 sk-*、ghp_*、AKIA*、AIza*、JWT、Bearer Token 等凭证格式", None::<&str>),
        ("b002", "credential.private_key", "credential", "critical",
         "私钥内容", "检测 PEM/OpenSSH 私钥头部", None),
        ("b003", "credential.named_secret", "credential", "high",
         "敏感凭证字段", "检测 Authorization、Cookie、Session、Secret 等字段名", None),
        ("b004", "credential.database_url", "credential", "high",
         "数据库连接串", "检测 mysql://、postgres://、mongodb://、redis:// 等连接串", None),
        ("b005", "credential.cloud_key", "credential", "high",
         "云厂商密钥", "检测 AWS AKIA、腾讯云 SecretId、阿里云 AccessKey 等", None),
        // ── 文件类（3条）──
        ("b006", "file.sensitive_path", "file", "high",
         "敏感文件路径", "检测 .env、~/.ssh、id_rsa、.aws/credentials 等敏感路径", None),
        ("b007", "file.ssh_key", "file", "critical",
         "SSH 密钥文件", "检测 id_rsa、id_ed25519、id_ecdsa 等 SSH 密钥文件名", None),
        ("b008", "file.cloud_credentials", "file", "high",
         "云凭证文件", "检测 .aws/credentials、.npmrc、.pypirc 等", None),
        // ── 基础设施（1条）──
        ("b009", "infra.local_path", "infra", "medium",
         "本地用户路径", "检测 /Users/、C:\\Users\\、/home/ 等本地路径", None),
        // ── 个人信息（2条）──
        ("b010", "personal.email", "personal", "low", "邮箱地址", "检测邮箱格式的字符串", None),
        ("b011", "personal.phone", "personal", "low", "手机号码", "检测中国大陆手机号格式", None),
        // ── Unicode 隐写（4条，受 scan_unicode 开关控制）──
        ("b012", "unicode.zero_width", "unicode", "medium",
         "零宽 Unicode 字符", "检测 U+200B/200C/200D/2060/FEFF 等不可见字符", Some("security.scan_unicode")),
        ("b013", "unicode.bidi_control", "unicode", "high",
         "方向控制 Unicode 字符", "检测 U+202A-202E 等 Bidi 控制字符", Some("security.scan_unicode")),
        ("b014", "unicode.variation_selector", "unicode", "medium",
         "变体选择符", "检测 variation selector", Some("security.scan_unicode")),
        ("b015", "unicode.homograph", "unicode", "medium",
         "同形异义字符", "检测西里尔、希腊等与拉丁字母同形的字符", Some("security.scan_unicode")),
        // ── 网络风险（4条，受 scan_network 开关控制）──
        ("b016", "network.ip_probe", "network", "high",
         "公网 IP 探测服务", "检测 ifconfig.me、ipinfo.io 等", Some("security.scan_network")),
        ("b017", "network.suspicious_domain", "network", "high",
         "可疑外联域名", "检测 webhook.site、ngrok、pastebin 等", Some("security.scan_network")),
        ("b018", "network.external_url", "network", "info",
         "外部 URL", "检测请求中的外部 URL 链接", Some("security.scan_network")),
        ("b019", "network.tracking_pixel", "network", "high",
         "追踪像素", "检测 1x1 图片、track/pixel/beacon 等追踪特征", Some("security.scan_network")),
        // ── 工具/命令（4条，受 scan_tools 开关控制）──
        ("b020", "tool.shell.network_or_exec", "tool", "medium",
         "高风险命令片段", "检测 curl/wget/nc/bash -c 等命令", Some("security.scan_tools")),
        ("b021", "tool.shell.exfiltration", "tool", "critical",
         "疑似敏感数据外传命令", "检测敏感文件读取 + 网络外传组合", Some("security.scan_tools")),
        ("b022", "tool.remote_script_exec", "tool", "critical",
         "远程脚本执行", "检测 curl|bash 下载执行组合", Some("security.scan_tools")),
        ("b023", "tool.git_info", "tool", "low",
         "Git 信息泄露", "检测 git remote、gh auth token 等", Some("security.scan_tools")),
        // ── 提示词风险（2条）──
        ("b024", "prompt.fingerprint_context", "prompt", "medium",
         "账号画像/风控上下文", "检测多个时区、代理、指纹、风控相关词同时出现", None),
        ("b025", "prompt.injection", "prompt", "high",
         "提示注入/越权", "检测要求模型忽略规则、隐藏行为、绕过审计等指令", None),
    ];

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    for (id, rule_id, category, severity, title, desc, toggle_key) in &seeds {
        // INSERT OR IGNORE：已存在则跳过，保证用户编辑不被覆盖
        sqlx::query(
            "INSERT OR IGNORE INTO security_builtin_rules
             (id, rule_id, category, severity, title, description, toggle_key, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)"
        )
        .bind(id).bind(rule_id).bind(category).bind(severity)
        .bind(title).bind(desc).bind(toggle_key).bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

// ---------- 内置规则 Repository ----------

pub struct BuiltinRuleRepository;

impl BuiltinRuleRepository {
    pub async fn get_all(pool: &SqlitePool) -> Result<Vec<BuiltinRule>, sqlx::Error> {
        sqlx::query_as::<_, BuiltinRule>("SELECT * FROM security_builtin_rules ORDER BY rule_id")
            .fetch_all(pool).await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        input: &UpdateBuiltinRuleInput,
    ) -> Result<(), sqlx::Error> {
        if let Some(enabled) = input.enabled {
            sqlx::query("UPDATE security_builtin_rules SET enabled = ? WHERE id = ?")
                .bind(if enabled { 1 } else { 0 }).bind(id)
                .execute(pool).await?;
        }
        if let Some(ref severity) = input.severity {
            sqlx::query("UPDATE security_builtin_rules SET severity = ? WHERE id = ?")
                .bind(severity).bind(id).execute(pool).await?;
        }
        if let Some(ref title) = input.title {
            sqlx::query("UPDATE security_builtin_rules SET title = ? WHERE id = ?")
                .bind(title).bind(id).execute(pool).await?;
        }
        if let Some(ref description) = input.description {
            sqlx::query("UPDATE security_builtin_rules SET description = ? WHERE id = ?")
                .bind(description).bind(id).execute(pool).await?;
        }
        Ok(())
    }

    pub async fn reset_to_defaults(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM security_builtin_rules").execute(pool).await?;
        seed_builtin_rules(pool).await?;
        Ok(())
    }
}

// ---------- 自定义规则 Repository ----------

pub struct CustomRuleRepository;

impl CustomRuleRepository {
    pub async fn get_all(pool: &SqlitePool) -> Result<Vec<CustomRule>, sqlx::Error> {
        sqlx::query_as::<_, CustomRule>(
            "SELECT * FROM security_custom_rules ORDER BY created_at DESC"
        )
        .fetch_all(pool).await
    }

    pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<CustomRule, sqlx::Error> {
        sqlx::query_as::<_, CustomRule>("SELECT * FROM security_custom_rules WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn create(
        pool: &SqlitePool,
        input: &CreateCustomRuleInput,
    ) -> Result<CustomRule, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        sqlx::query(
            "INSERT INTO security_custom_rules
             (id, rule_type, category, pattern, severity, action, enabled, description, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)"
        )
        .bind(&id).bind(&input.rule_type).bind(&input.category).bind(&input.pattern)
        .bind(input.severity.as_deref().unwrap_or("medium"))
        .bind(input.action.as_deref().unwrap_or("warn"))
        .bind(&input.description).bind(&now)
        .execute(pool).await?;
        // 返回创建的记录
        sqlx::query_as::<_, CustomRule>("SELECT * FROM security_custom_rules WHERE id = ?")
            .bind(&id).fetch_one(pool).await
    }

    pub async fn update_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE security_custom_rules SET enabled = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM security_custom_rules WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

// ---------- 自定义规则应用 ----------

/// 字符串严重度 → RiskLevel
fn parse_severity(severity: &str) -> super::models::RiskLevel {
    match severity {
        "critical" => super::models::RiskLevel::Critical,
        "high" => super::models::RiskLevel::High,
        "medium" => super::models::RiskLevel::Medium,
        "low" => super::models::RiskLevel::Low,
        "info" => super::models::RiskLevel::Info,
        _ => super::models::RiskLevel::Medium,
    }
}

/// 黑名单匹配：命中即产生 finding
pub fn apply_custom_rules(
    text: &str,
    phase: &str,
    location: &str,
    custom_rules: &[CustomRule],
    findings: &mut Vec<SecurityFinding>,
) {
    let lower = text.to_ascii_lowercase();
    for rule in custom_rules {
        if rule.enabled == 0 { continue; }
        let pattern_lower = rule.pattern.to_ascii_lowercase();
        let matched = match rule.category.as_str() {
            "domain" | "tool" | "path" | "keyword" => lower.contains(&pattern_lower),
            _ => false,
        };
        if matched && rule.rule_type == "blacklist" {
            let severity = parse_severity(&rule.severity);
            scanner::add_finding(
                findings, phase, "custom",
                &format!("custom.{}.{}", rule.rule_type, rule.category),
                severity,
                &format!("自定义规则: {}", rule.category),
                &rule.description.clone().unwrap_or_else(||
                    format!("匹配自定义{}规则: {}", rule.rule_type, rule.pattern)),
                location, &rule.pattern,
            );
        }
    }
}

/// 白名单检查：命中则豁免对应类别的检测
pub fn is_whitelisted(category: &str, value: &str, custom_rules: &[CustomRule]) -> bool {
    let lower = value.to_ascii_lowercase();
    custom_rules.iter().any(|r| {
        r.enabled == 1
            && r.rule_type == "whitelist"
            && r.category == category
            && lower.contains(&r.pattern.to_ascii_lowercase())
    })
}
