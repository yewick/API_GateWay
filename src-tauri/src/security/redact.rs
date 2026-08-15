use super::models::SecuritySettings;

pub fn redact_json(value: &serde_json::Value, settings: &SecuritySettings) -> serde_json::Value {
    if !settings.enabled || !settings.redact_secrets {
        return value.clone();
    }
    let mut cloned = value.clone();
    redact_value_in_place(&mut cloned);
    cloned
}

fn redact_value_in_place(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = redact_string(s);
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_value_in_place(item);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                // 键名敏感的直接打码整个值
                let lower_key = k.to_ascii_lowercase();
                if is_secret_field(&lower_key) {
                    if let Some(s) = v.as_str() {
                        if s.len() > 4 {
                            *v = serde_json::Value::String(mask_string(s));
                        }
                    }
                }
                redact_value_in_place(v);
            }
        }
        _ => {}
    }
}

/// 敏感键名列表
fn is_secret_field(key: &str) -> bool {
    matches!(key,
        "api_key" | "apikey" | "secret" | "secret_key" | "access_key"
        | "access_token" | "auth_token" | "token" | "password" | "passwd"
        | "authorization" | "cookie" | "session" | "sessionid" | "private_key"
        | "client_secret" | "aws_secret_access_key" | "secretkey"
    )
}

fn redact_string(s: &str) -> String {
    let mut result = s.to_string();
    // 1. 脱敏各类 API Key（前缀模式匹配）
    result = redact_pattern(&result, |t| {
        (t.starts_with("sk-") && t.len() >= 24)
            || (t.starts_with("sk-ant-") && t.len() >= 30)
            || (t.starts_with("ghp_") && t.len() >= 20)
            || (t.starts_with("AKIA") && t.len() >= 16)
            || (t.starts_with("AIza") && t.len() >= 20)
    });
    // 2. 脱敏 JWT
    result = redact_pattern(&result, |t| {
        t.starts_with("eyJ") && t.len() >= 30 && t.contains('.')
    });
    // 3. 脱敏 Bearer Token（保留前7字符示意，如 "Bearer a****z"）
    result = redact_bearer_tokens(&result);

    // 4. 脱敏私钥（整个 PEM 块替换为占位标记）
    result = redact_pem_blocks(&result);

    result
}

/// 脱敏 Bearer Token：匹配 "Bearer <token>" 模式，保留前缀和 token 首尾各2字符
fn redact_bearer_tokens(text: &str) -> String {
    // 大小写不敏感匹配 "bearer " 前缀
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();

    while i < bytes.len() {
        // 检查当前位置是否是 "bearer "（不区分大小写）
        let remaining = bytes.len() - i;
        if remaining >= 7 {
            let slice = &bytes[i..i + 7];
            let slice_lower: Vec<u8> = slice.iter().map(|b| b.to_ascii_lowercase()).collect();
            if &slice_lower == b"bearer " {
                // 找到 "bearer "，保留原文字
                result.push_str(&text[i..i + 7]);
                i += 7;

                // 收集 token 部分（直到遇到空白或结束）
                let token_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let token = &text[token_start..i];
                if token.len() > 6 {
                    let prefix: String = token.chars().take(2).collect();
                    let suffix: String = token.chars().rev().take(2)
                        .collect::<Vec<_>>().iter().rev().collect();
                    result.push_str(&format!("{}****{}", prefix, suffix));
                } else {
                    result.push_str("****");
                }
                continue;
            }
        }
        result.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    result
}

/// 脱敏 PEM 块：将 -----BEGIN ...----- 到 -----END ...----- 之间的内容替换
fn redact_pem_blocks(text: &str) -> String {
    let begin_markers = [
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN DSA PRIVATE KEY-----",
        "-----BEGIN CERTIFICATE-----",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ];
    let end_markers = [
        "-----END OPENSSH PRIVATE KEY-----",
        "-----END RSA PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
        "-----END EC PRIVATE KEY-----",
        "-----END DSA PRIVATE KEY-----",
        "-----END CERTIFICATE-----",
        "-----END ENCRYPTED PRIVATE KEY-----",
    ];

    let mut result = text.to_string();

    for (begin, end) in begin_markers.iter().zip(end_markers.iter()) {
        loop {
            let lower = result.to_ascii_lowercase();
            let begin_lower = begin.to_ascii_lowercase();
            let end_lower = end.to_ascii_lowercase();

            if let Some(start_idx) = lower.find(&begin_lower) {
                if let Some(end_idx) = lower[start_idx..].find(&end_lower) {
                    let end_pos = start_idx + end_idx + end.len();
                    // 保留头部和尾部标记，中间内容替换为 [REDACTED]
                    let before = result[..start_idx + begin.len()].to_string();
                    let after = result[end_pos..].to_string();
                    result = format!("{}\n[REDACTED PRIVATE KEY BLOCK]\n{}", before, after);
                    continue;
                }
            }
            break;
        }
    }

    result
}

/// 通用模式脱敏：按分隔符切词，命中模式的词替换为 mask
fn redact_pattern<F>(text: &str, matcher: F) -> String
where F: Fn(&str) -> bool {
    let mut result = String::with_capacity(text.len());
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || ch == ':' {
            current.push(ch);
        } else {
            if !current.is_empty() {
                if matcher(&current) {
                    result.push_str(&mask_string(&current));
                } else {
                    result.push_str(&current);
                }
                current.clear();
            }
            result.push(ch);
        }
    }
    if !current.is_empty() {
        if matcher(&current) {
            result.push_str(&mask_string(&current));
        } else {
            result.push_str(&current);
        }
    }
    result
}

pub fn mask_string(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    // 保留头4尾4：sk-1****cdef
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars.iter().rev().take(4).copied()
        .collect::<Vec<_>>().iter().rev().collect();
    format!("{}****{}", prefix, suffix)
}