// ===== 扫描引擎 =====
// 对 JSON 请求/响应体做深度遍历，检测凭证泄露、Unicode 隐写、工具/网络风险。

use super::models::{RiskLevel, SecurityAction, SecurityFinding, SecurityScanResult, SecuritySettings};
use super::rules;
use serde_json::Value;
use std::collections::HashSet;

/// 单次扫描的 finding 上限（防恶意构造超大量 finding）
const MAX_FINDINGS: usize = 80;

// ---------- JSON 递归遍历 ----------

pub fn scan_json(
    value: &Value,
    phase: &str,
    settings: &SecuritySettings,
    custom_rules: Option<&[crate::security::CustomRule]>,
    disabled_builtin: &HashSet<String>,
) -> SecurityScanResult {
    let mut findings: Vec<SecurityFinding> = Vec::new();
    walk_json(value, phase, "$", settings, custom_rules, &mut findings);

    // 过滤被禁用的内置规则（enabled=0，仅抑制 finding，不影响自定义规则）
    if !disabled_builtin.is_empty() {
        findings.retain(|f| !disabled_builtin.contains(&f.rule_id));
    }

    // 评分计算
    compute_score(&findings)
}

fn walk_json(
    value: &Value,
    phase: &str,
    path: &str,
    settings: &SecuritySettings,
    custom_rules: Option<&[crate::security::CustomRule]>,
    findings: &mut Vec<SecurityFinding>,
) {
    if findings.len() >= MAX_FINDINGS {
        return;
    }
    match value {
        Value::String(s) => scan_text(s, phase, path, settings, custom_rules, findings),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_json(item, phase, &format!("{}[{}]", path, i), settings, custom_rules, findings);
                if findings.len() >= MAX_FINDINGS {
                    break;
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let child = if path == "$" {
                    format!("$.{}", k)
                } else {
                    format!("{}.{}", path, k)
                };
                walk_json(v, phase, &child, settings, custom_rules, findings);
                if findings.len() >= MAX_FINDINGS {
                    break;
                }
            }
        }
        _ => {} // 数字、布尔、null 无需扫描
    }
}

/// 对单个文本字段执行全部检测
fn scan_text(
    text: &str,
    phase: &str,
    location: &str,
    settings: &SecuritySettings,
    custom_rules: Option<&[crate::security::CustomRule]>,
    findings: &mut Vec<SecurityFinding>,
) {
    // 截断保护：超长字段只扫前 N 字节
    let s = if text.len() > settings.max_scan_bytes {
        &text[..settings.max_scan_bytes]
    } else {
        text
    };

    // 先写入局部集合，便于白名单按 finding 粒度豁免
    let mut local: Vec<SecurityFinding> = Vec::new();

    scan_credentials(s, phase, location, &mut local);
    if settings.scan_unicode {
        scan_unicode(s, phase, location, &mut local);
    }
    if settings.scan_tools {
        scan_tool_risks(s, phase, location, &mut local);
    }
    if settings.scan_network {
        scan_network(s, phase, location, &mut local);
    }
    scan_file_and_personal(s, phase, location, &mut local);

    // 应用自定义规则（黑名单 + 白名单）
    if let Some(rules) = custom_rules {
        if !rules.is_empty() {
            // 黑名单：命中即产生 finding
            rules::apply_custom_rules(s, phase, location, rules, &mut local);

            // 白名单：按 category 映射豁免对应内置检测类别（不豁免 custom 黑名单）
            let lower = s.to_ascii_lowercase();
            let mut exempt_all = false;
            let mut exempt_categories: Vec<&str> = Vec::new();
            for r in rules
                .iter()
                .filter(|r| r.enabled == 1 && r.rule_type == "whitelist")
            {
                if lower.contains(&r.pattern.to_ascii_lowercase()) {
                    match r.category.as_str() {
                        "domain" => exempt_categories.push("network"),
                        "tool" => exempt_categories.push("tool"),
                        "path" => {
                            exempt_categories.push("file");
                            exempt_categories.push("infra");
                        }
                        "keyword" => exempt_all = true,
                        _ => {}
                    }
                }
            }
            if exempt_all {
                local.retain(|f| f.category == "custom");
            } else if !exempt_categories.is_empty() {
                local.retain(|f| {
                    f.category == "custom" || !exempt_categories.contains(&f.category.as_str())
                });
            }
        }
    }

    findings.extend(local);
}

// ---------- 5.2 凭证检测 ----------

/// 按分隔符切分候选 token
fn split_candidates(text: &str) -> Vec<&str> {
    let delimiters: &[char] = &[' ', '\n', '\t', '"', '\'', ':', ';', ',', '(', ')', '`', '=', '<', '>'];
    text.split(|c: char| delimiters.contains(&c))
        .filter(|s| !s.is_empty())
        .collect()
}

fn scan_credentials(
    text: &str,
    phase: &str,
    location: &str,
    findings: &mut Vec<SecurityFinding>,
) {
    // 按分隔符切分候选 token
    for token in split_candidates(text) {
        let t = token.trim_matches(|c: char| "\"',;()`".contains(c));
        let lower = t.to_ascii_lowercase();
        let is_secret = (t.starts_with("sk-") && t.len() >= 24) ||       // OpenAI 风格
            (t.starts_with("sk-ant-") && t.len() >= 30) ||  // Anthropic
            (t.starts_with("sk-proj-") && t.len() >= 24) || // OpenAI Project
            (t.starts_with("ghp_") && t.len() >= 20) ||     // GitHub PAT
            (t.starts_with("gho_") && t.len() >= 20) ||     // GitHub OAuth
            (t.starts_with("xoxb-") && t.len() >= 20) ||    // Slack Bot
            (t.starts_with("AKIA") && t.len() >= 16) ||     // AWS Access Key
            (t.starts_with("AIza") && t.len() >= 20) ||     // Google API Key
            (t.starts_with("eyJ") && t.len() >= 30 && t.contains('.')) || // JWT
            lower.starts_with("bearer ");
        if is_secret {
            add_finding(
                findings,
                phase,
                "credential",
                "credential.secret_token",
                RiskLevel::High,
                "发现疑似密钥/Token",
                "请求内容中出现 API Key、Bearer Token、GitHub Token、JWT 或云厂商密钥样式字符串。",
                location,
                &mask_evidence(t),
            );
            break; // 一个字段报一次即可
        }
    }

    // PEM 私钥：最严重等级
    if text.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
        || text.contains("-----BEGIN RSA PRIVATE KEY-----")
        || text.contains("-----BEGIN PRIVATE KEY-----")
    {
        add_finding(
            findings,
            phase,
            "credential",
            "credential.private_key",
            RiskLevel::Critical,
            "发现私钥内容",
            "请求内容中包含私钥 PEM/OpenSSH 头部，存在严重凭证泄露风险。",
            location,
            "-----BEGIN PRIVATE KEY-----",
        );
    }

    // 敏感字段名
    let lower = text.to_ascii_lowercase();
    for key in [
        "authorization:",
        "cookie:",
        "sessionid=",
        "auth_token=",
        "secret_key",
        "access_key",
        "database_url",
    ] {
        if lower.contains(key) {
            add_finding(
                findings,
                phase,
                "credential",
                "credential.named_secret",
                RiskLevel::High,
                "发现敏感凭证字段",
                "请求内容包含 Authorization、Cookie、Session 或 Secret 字段名。",
                location,
                key,
            );
            break;
        }
    }

    // 数据库连接串（mysql://、postgres://、mongodb://、redis:// 等）
    let db_url = [
        "mysql://",
        "postgres://",
        "postgresql://",
        "mongodb://",
        "redis://",
    ];
    if db_url.iter().any(|x| lower.contains(x)) {
        add_finding(
            findings,
            phase,
            "credential",
            "credential.database_url",
            RiskLevel::High,
            "发现数据库连接串",
            "内容包含数据库连接串（mysql/postgres/mongodb/redis），存在凭证泄露风险。",
            location,
            &snippet(text),
        );
    }

    // 云厂商密钥（腾讯云 SecretId、阿里云 AccessKey）
    if lower.contains("akid") || lower.contains("ltai") {
        add_finding(
            findings,
            phase,
            "credential",
            "credential.cloud_key",
            RiskLevel::High,
            "发现云厂商密钥",
            "内容包含腾讯云 SecretId 或阿里云 AccessKey 样式。",
            location,
            &snippet(text),
        );
    }
}

// ---------- 5.3 Unicode 隐写检测 ----------

fn scan_unicode(
    text: &str,
    phase: &str,
    location: &str,
    findings: &mut Vec<SecurityFinding>,
) {
    let mut zero_width = 0u32;
    let mut bidi = 0u32;
    let mut variation = 0u32;
    let mut homograph = 0u32;
    for ch in text.chars() {
        let code = ch as u32;
        // 零宽字符：ZWSP/ZWNJ/ZWJ/WORD JOINER/BOM
        if matches!(code, 0x200B | 0x200C | 0x200D | 0x2060 | 0xFEFF) {
            zero_width += 1;
        }
        // Bidi 方向控制：可以改变文本的视觉显示顺序（Trojan Source 攻击）
        if (0x202A..=0x202E).contains(&code) || (0x2066..=0x2069).contains(&code) {
            bidi += 1;
        }
        // 变体选择符：可用于隐写编码
        if (0xFE00..=0xFE0F).contains(&code) || (0xE0100..=0xE01EF).contains(&code) {
            variation += 1;
        }
        // 同形异义字符：西里尔/希腊字母
        if (0x0400..=0x04FF).contains(&code) || (0x0370..=0x03FF).contains(&code) {
            homograph += 1;
        }
    }
    if zero_width > 0 {
        add_finding(
            findings,
            phase,
            "unicode",
            "unicode.zero_width",
            RiskLevel::Medium,
            "发现零宽 Unicode 字符",
            "内容包含不可见零宽字符，可能用于隐藏标记或混淆文本。",
            location,
            &format!("zero_width_count={}", zero_width),
        );
    }
    if bidi > 0 {
        add_finding(
            findings,
            phase,
            "unicode",
            "unicode.bidi_control",
            RiskLevel::High,
            "发现方向控制 Unicode 字符",
            "内容包含 Bidi 方向控制字符，可能改变代码、URL 或命令的视觉顺序。",
            location,
            &format!("bidi_count={}", bidi),
        );
    }
    if variation > 0 {
        add_finding(
            findings,
            phase,
            "unicode",
            "unicode.variation_selector",
            RiskLevel::Low,
            "发现变体选择符",
            "内容包含 Unicode 变体选择符，可能用于隐写编码。",
            location,
            &format!("variation_count={}", variation),
        );
    }
    if homograph > 0 {
        add_finding(
            findings,
            phase,
            "unicode",
            "unicode.homograph",
            RiskLevel::Medium,
            "发现同形异义字符",
            "内容包含西里尔/希腊等同形字符，可能用于混淆或钓鱼。",
            location,
            &format!("homograph_count={}", homograph),
        );
    }
}

// ---------- 5.4 网络与工具风险检测 ----------

fn scan_network(
    text: &str,
    phase: &str,
    location: &str,
    findings: &mut Vec<SecurityFinding>,
) {
    let lower = text.to_ascii_lowercase();

    // 公网 IP 探测服务
    let ip_probe = [
        "ifconfig.me",
        "ipinfo.io",
        "ip-api.com",
        "ipify.org",
        "icanhazip.com",
    ];
    if ip_probe.iter().any(|x| lower.contains(x)) {
        add_finding(
            findings,
            phase,
            "network",
            "network.ip_probe",
            RiskLevel::High,
            "检测到 IP 探测服务",
            "内容包含公网 IP 查询服务地址，可能用于泄露服务器 IP。",
            location,
            &snippet(text),
        );
    }

    // 数据接收常用域名
    let suspicious = [
        "webhook.site",
        "requestbin",
        "ngrok",
        "pastebin.com",
        "transfer.sh",
    ];
    if suspicious.iter().any(|x| lower.contains(x)) {
        add_finding(
            findings,
            phase,
            "network",
            "network.suspicious_domain",
            RiskLevel::Medium,
            "检测到可疑外部域名",
            "内容包含常用于数据接收的可疑域名（webhook/bin/ngrok/pastebin）。",
            location,
            &snippet(text),
        );
    }

    // 外部 URL 提示（Info 级）
    if lower.contains("http://") || lower.contains("https://") {
        add_finding(
            findings,
            phase,
            "network",
            "network.external_url",
            RiskLevel::Info,
            "检测到外部 URL",
            "内容包含外部 URL 链接。",
            location,
            &snippet(text),
        );
    }

    // 追踪像素（1x1 图片、track/pixel/beacon 特征）
    let tracking = ["1x1", "tracking", "pixel", "beacon"];
    if tracking.iter().any(|x| lower.contains(x)) {
        add_finding(
            findings,
            phase,
            "network",
            "network.tracking_pixel",
            RiskLevel::High,
            "检测到追踪像素",
            "内容包含 1x1 图片或 track/pixel/beacon 等追踪特征。",
            location,
            &snippet(text),
        );
    }
}

fn scan_tool_risks(
    text: &str,
    phase: &str,
    location: &str,
    findings: &mut Vec<SecurityFinding>,
) {
    let lower = text.to_ascii_lowercase();

    // 高风险命令：curl/wget/nc/scp/bash -c/python -c/powershell
    let has_shell = [
        "curl ",
        "wget ",
        " nc ",
        "bash -c",
        "python -c",
        "powershell",
    ]
    .iter()
    .any(|x| lower.contains(x));

    if has_shell {
        add_finding(
            findings,
            phase,
            "tool",
            "tool.shell.command",
            RiskLevel::Medium,
            "检测到命令行执行特征",
            "内容包含命令行执行特征（curl/wget/nc/bash -c 等），可能存在命令注入风险。",
            location,
            &snippet(text),
        );
    }

    // 组合检测：敏感文件读取 + 网络外发 = 数据外传（Critical）
    let reads_sensitive = [
        "cat /etc/passwd",
        "cat /etc/shadow",
        "cat .env",
        "cat ~/.ssh",
        "printenv",
        "base64 ~/.ssh",
        "/etc/passwd",
        "/etc/shadow",
        ".ssh/id_rsa",
        ".aws/credentials",
    ]
    .iter()
    .any(|x| lower.contains(x));

    let network = ["curl ", "wget ", "http://", "https://"]
        .iter()
        .any(|x| lower.contains(x));

    if reads_sensitive && network {
        add_finding(
            findings,
            phase,
            "tool",
            "tool.shell.exfiltration",
            RiskLevel::Critical,
            "疑似敏感数据外传命令",
            "内容同时包含敏感文件/环境变量读取与外部网络传输特征，存在严重外泄风险。",
            location,
            &snippet(text),
        );
    }

    // 敏感文件路径检测
    if reads_sensitive {
        add_finding(
            findings,
            phase,
            "file",
            "file.sensitive_path",
            RiskLevel::High,
            "检测到敏感文件路径",
            "内容包含敏感系统文件路径（/etc/passwd, .ssh, .aws 等）。",
            location,
            &snippet(text),
        );
    }

    // 远程脚本执行：curl/wget 下载并通过管道交给 shell 执行
    let download_exec = (lower.contains("curl") || lower.contains("wget"))
        && lower.contains("|")
        && (lower.contains("bash")
            || lower.contains("sh -c")
            || lower.contains("sh;")
            || lower.contains("sh &&"));
    if download_exec {
        add_finding(
            findings,
            phase,
            "tool",
            "tool.remote_script_exec",
            RiskLevel::Critical,
            "远程脚本执行",
            "内容包含 curl/wget 下载并通过管道交给 shell 执行的组合，存在远程代码执行风险。",
            location,
            &snippet(text),
        );
    }

    // Git 信息泄露
    if lower.contains("git remote") || lower.contains("gh auth token") || lower.contains(".git/config") {
        add_finding(
            findings,
            phase,
            "tool",
            "tool.git_info",
            RiskLevel::Low,
            "Git 信息泄露",
            "内容包含 git remote、gh auth token 等 Git 相关信息。",
            location,
            &snippet(text),
        );
    }
}

// ---------- 5.4b 文件/基础设施/个人信息检测 ----------

fn scan_file_and_personal(
    text: &str,
    phase: &str,
    location: &str,
    findings: &mut Vec<SecurityFinding>,
) {
    let lower = text.to_ascii_lowercase();

    // SSH 密钥文件
    if lower.contains("id_rsa") || lower.contains("id_ed25519") || lower.contains("id_ecdsa") {
        add_finding(
            findings,
            phase,
            "file",
            "file.ssh_key",
            RiskLevel::Critical,
            "SSH 密钥文件",
            "内容包含 SSH 私钥文件名（id_rsa/id_ed25519/id_ecdsa）。",
            location,
            &snippet(text),
        );
    }

    // 云凭证文件
    let cloud_cred = [".aws/credentials", ".npmrc", ".pypirc"];
    if cloud_cred.iter().any(|x| lower.contains(x)) {
        add_finding(
            findings,
            phase,
            "file",
            "file.cloud_credentials",
            RiskLevel::High,
            "云凭证文件",
            "内容包含云凭证文件路径（.aws/credentials、.npmrc、.pypirc）。",
            location,
            &snippet(text),
        );
    }

    // 本地用户路径
    if lower.contains("/users/") || lower.contains("c:\\users\\") || lower.contains("/home/") {
        add_finding(
            findings,
            phase,
            "infra",
            "infra.local_path",
            RiskLevel::Medium,
            "本地用户路径",
            "内容包含本地用户绝对路径（/Users/、C:\\Users\\、/home/）。",
            location,
            &snippet(text),
        );
    }

    // 邮箱地址
    if is_email(text) {
        add_finding(
            findings,
            phase,
            "personal",
            "personal.email",
            RiskLevel::Low,
            "邮箱地址",
            "内容包含邮箱格式的字符串。",
            location,
            &snippet(text),
        );
    }

    // 手机号码（中国大陆）
    if is_phone(text) {
        add_finding(
            findings,
            phase,
            "personal",
            "personal.phone",
            RiskLevel::Low,
            "手机号码",
            "内容包含中国大陆手机号格式。",
            location,
            &snippet(text),
        );
    }
}

fn is_email(text: &str) -> bool {
    split_candidates(text).iter().any(|t| {
        let t = t.trim_matches(|c: char| "\"',;()`".contains(c));
        if let Some(at) = t.find('@') {
            let local = &t[..at];
            let domain = &t[at + 1..];
            !local.is_empty() && domain.contains('.') && domain.len() >= 3
        } else {
            false
        }
    })
}

fn is_phone(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'1' && i + 11 <= bytes.len() {
            let candidate = &bytes[i + 1..i + 11];
            let all_digit = candidate.iter().all(|b| b.is_ascii_digit());
            let second = bytes[i + 1];
            if all_digit && (b'3'..=b'9').contains(&second) {
                let before_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
                let after_ok = i + 11 >= bytes.len() || !bytes[i + 11].is_ascii_digit();
                if before_ok && after_ok {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

// ---------- 5.5 评分系统 ----------

fn compute_score(findings: &[SecurityFinding]) -> SecurityScanResult {
    if findings.is_empty() {
        return SecurityScanResult {
            risk_level: RiskLevel::Clean,
            risk_score: 0,
            action: SecurityAction::Allow,
            sanitized: false,
            blocked_reason: None,
            summary: "未发现安全风险".to_string(),
            findings: vec![],
        };
    }

    // 基础分：取单条最高严重度
    let mut score = 0i32;
    let mut max_level = RiskLevel::Clean;
    for f in findings {
        let base = match f.severity {
            RiskLevel::Clean => 0,
            RiskLevel::Info => 5,
            RiskLevel::Low => 15,
            RiskLevel::Medium => 35,
            RiskLevel::High => 65,
            RiskLevel::Critical => 90,
        };
        score = score.max(base);
        if f.severity.rank() > max_level.rank() {
            max_level = f.severity.clone();
        }
    }

    // 组合加分：多类风险同时出现，危险不是线性叠加而是指数增长
    let has_credential = findings.iter().any(|f| f.category == "credential");
    let has_network = findings.iter().any(|f| f.category == "network");
    let has_sensitive_file = findings
        .iter()
        .any(|f| f.rule_id == "file.sensitive_path");
    let has_unicode = findings.iter().any(|f| f.category == "unicode");
    let has_shell = findings
        .iter()
        .any(|f| f.rule_id.starts_with("tool.shell"));

    if has_credential && has_network {
        score += 25;
    } // 凭证 + 外联 = 泄露中
    if has_sensitive_file && has_network {
        score += 25;
    } // 敏感文件 + 外联
    if has_unicode && has_network {
        score += 15;
    } // 隐写 + 外联
    if has_shell && has_sensitive_file {
        score += 20;
    } // 命令 + 敏感文件
    score = score.min(100);

    // 分数反推等级（升级不降级）
    if score >= 90 {
        max_level = RiskLevel::Critical;
    } else if score >= 65 && max_level.rank() < RiskLevel::High.rank() {
        max_level = RiskLevel::High;
    } else if score >= 35 && max_level.rank() < RiskLevel::Medium.rank() {
        max_level = RiskLevel::Medium;
    }

    let summary = summarize(findings, &max_level);
    let findings_clone = findings.to_vec();

    SecurityScanResult {
        risk_level: max_level,
        risk_score: score,
        action: SecurityAction::Allow, // 由 decide_action 覆盖
        sanitized: false,
        blocked_reason: None,
        summary,
        findings: findings_clone,
    }
}

fn summarize(findings: &[SecurityFinding], max_level: &RiskLevel) -> String {
    let count = findings.len();
    let mut cats: Vec<&str> = findings.iter().map(|f| f.category.as_str()).collect();
    cats.sort_unstable();
    cats.dedup();
    format!(
        "检测到 {} 条风险规则命中，涉及类别：{}（最高等级：{}）",
        count,
        cats.join(", "),
        max_level.as_str(),
    )
}

// ---------- 5.6 证据打码 ----------

fn mask_evidence(e: &str) -> String {
    let s = e.replace('\n', " ");
    if s.len() <= 16 {
        return s;
    }
    let start: String = s.chars().take(8).collect();
    let end: String = s
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}****{}", start, end)
}
// sk-ant-api03-xxxxx... → "sk-ant-a****x123"

// ---------- 工具函数 ----------

/// 截取文本片段（用于 evidence）
fn snippet(text: &str) -> String {
    if text.len() <= 64 {
        text.to_string()
    } else {
        format!("{}...", &text[..64])
    }
}

/// 添加一条 finding（带上限保护）。公开以支持 custom rules 调用。
pub fn add_finding(
    findings: &mut Vec<SecurityFinding>,
    phase: &str,
    category: &str,
    rule_id: &str,
    severity: RiskLevel,
    title: &str,
    description: &str,
    location: &str,
    evidence_masked: &str,
) {
    if findings.len() >= MAX_FINDINGS {
        return;
    }
    findings.push(SecurityFinding {
        phase: phase.to_string(),
        category: category.to_string(),
        rule_id: rule_id.to_string(),
        severity,
        action: "warn".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        location: location.to_string(),
        evidence_masked: evidence_masked.to_string(),
    });
}
