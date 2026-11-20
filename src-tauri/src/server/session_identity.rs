//! 从客户端原始请求中提取会话身份。
//!
//! 只在写入日志时使用，不参与协议转换。优先级：
//! - Responses / Codex: prompt_cache_key → client_metadata.session_id/thread_id
//! - Messages / Claude Code: x-claude-code-session-id → metadata.user_id 内的 session_id
//! - Chat / 其他: session 类请求头 → metadata.session_id → 请求体前缀指纹
//!
//! 绝不生成「每请求随机 UUID」作为会话键，否则会把一个会话拆成无数个。

use axum::http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Chat,
    Messages,
    Responses,
}

impl Protocol {
    pub fn client(self) -> &'static str {
        match self {
            Protocol::Chat => "chat",
            Protocol::Messages => "claude-code",
            Protocol::Responses => "codex",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub session_key: Option<String>,
    pub client_session_id: Option<String>,
    pub session_source: String,
    pub session_client: Option<String>,
    pub session_title: Option<String>,
    pub request_kind: String,
    pub history_len: Option<i64>,
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self {
            session_key: None,
            client_session_id: None,
            session_source: "none".to_string(),
            session_client: None,
            session_title: None,
            request_kind: "turn".to_string(),
            history_len: None,
        }
    }
}

impl SessionMeta {
    pub fn none() -> Self {
        Self::default()
    }

    /// 写入 `RequestLog` 的会话字段。
    pub fn apply(&self, log: &mut crate::db::models::RequestLog) {
        log.session_key = self.session_key.clone();
        log.client_session_id = self.client_session_id.clone();
        log.session_source = self.session_source.clone();
        log.session_client = self.session_client.clone();
        log.session_title = self.session_title.clone();
        log.request_kind = self.request_kind.clone();
        log.history_len = self.history_len;
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn header_value(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn strip_tag(mut text: String, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    loop {
        let Some(start) = text.find(&open) else {
            return text;
        };
        let Some(end_rel) = text[start..].find(&close) else {
            // 未闭合标签：保留内容，避免误删整段。
            return text;
        };
        let end = start + end_rel + close.len();
        text.replace_range(start..end, " ");
    }
}

/// 去掉环境注入内容并压缩空白；返回 None 表示这条消息不是真实用户输入。
fn clean_user_text(raw: &str) -> Option<String> {
    let mut text = raw.to_string();
    for tag in ["environment_context", "user_instructions", "system-reminder"] {
        text = strip_tag(text, tag);
    }
    let text = text.replace("[Request interrupted by user]", " ");
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

fn content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => clean_user_text(text),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                    if matches!(
                        kind,
                        "tool_use" | "tool_result" | "thinking" | "image" | "input_image"
                            | "input_file" | "reasoning"
                    ) {
                        return None;
                    }
                    item.get("text")
                        .and_then(Value::as_str)
                        .and_then(clean_user_text)
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Value::Object(object) => object
            .get("text")
            .and_then(Value::as_str)
            .and_then(clean_user_text),
        _ => None,
    }
}

fn chat_first_user(body: &Value) -> Option<String> {
    body.get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages.iter().find_map(|message| {
                if message.get("role").and_then(Value::as_str) != Some("user") {
                    return None;
                }
                message.get("content").and_then(content_text)
            })
        })
}

fn responses_first_user(body: &Value) -> Option<String> {
    let input = body.get("input")?;
    if let Some(text) = input.as_str() {
        return clean_user_text(text);
    }
    input.as_array().and_then(|items| {
        items.iter().find_map(|item| {
            let kind = item.get("type").and_then(Value::as_str).unwrap_or("message");
            if kind != "message" {
                return None;
            }
            if item.get("role").and_then(Value::as_str) != Some("user") {
                return None;
            }
            item.get("content").and_then(content_text)
        })
    })
}

fn first_user_text(protocol: Protocol, body: &Value) -> Option<String> {
    match protocol {
        Protocol::Chat | Protocol::Messages => chat_first_user(body),
        Protocol::Responses => responses_first_user(body),
    }
}

fn history_len(protocol: Protocol, body: &Value) -> Option<i64> {
    match protocol {
        Protocol::Chat | Protocol::Messages => body
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| messages.len() as i64),
        Protocol::Responses => match body.get("input") {
            Some(Value::Array(items)) => Some(items.len() as i64),
            Some(Value::String(_)) => Some(1),
            _ => None,
        },
    }
}

fn is_auxiliary_prompt(text: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "[SUGGESTION MODE",
        "The following is the user's CLAUDE.md configuration",
        "Perform a web search for the query:",
        "<transcript>",
        "Generate a title",
        "Summarize the following conversation",
    ];
    PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix) || text.starts_with(&format!("{prefix} ")))
}

fn codex_turn_metadata(body: &Value) -> Option<Value> {
    let raw = body
        .get("client_metadata")
        .and_then(|meta| meta.get("x-codex-turn-metadata"))
        .and_then(Value::as_str)?;
    serde_json::from_str(raw).ok()
}

fn session_title(text: Option<&str>) -> Option<String> {
    let text = text?;
    if is_auxiliary_prompt(text) {
        return None;
    }
    let mut title: String = text.chars().take(80).collect();
    if text.chars().count() > 80 {
        title.push('…');
    }
    Some(title)
}

/// 从请求头 / 原始请求体提取会话身份。`api_key_id` 只用于前缀兜底命名空间。
pub fn extract(
    headers: &HeaderMap,
    body: &Value,
    protocol: Protocol,
    api_key_id: &str,
) -> SessionMeta {
    let user_text = first_user_text(protocol, body);
    let mut request_kind = "turn".to_string();

    // 协议内显式会话标识。
    let (client_session_id, source) = match protocol {
        Protocol::Responses => {
            let cache_key = body
                .get("prompt_cache_key")
                .and_then(Value::as_str)
                .filter(|value| value.len() >= 8)
                .map(str::to_string);
            let meta_session = body
                .get("client_metadata")
                .and_then(|meta| meta.get("session_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let thread_id = body
                .get("client_metadata")
                .and_then(|meta| meta.get("thread_id"))
                .and_then(Value::as_str)
                .map(str::to_string);

            if let Some(meta) = codex_turn_metadata(body) {
                let kind = meta
                    .get("request_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("turn");
                let trigger = meta
                    .get("turn_trigger")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if kind != "turn" || trigger == "thread_title" {
                    request_kind = "auxiliary".to_string();
                }
            }

            if let Some(id) = cache_key {
                (Some(id), "prompt_cache_key")
            } else if let Some(id) = meta_session {
                (Some(id), "client_metadata")
            } else if let Some(id) = thread_id {
                (Some(id), "client_metadata")
            } else {
                (None, "none")
            }
        }
        Protocol::Messages => {
            let from_header = header_value(
                headers,
                &["x-claude-code-session-id", "claude-code-session-id"],
            );
            if let Some(id) = from_header {
                (Some(id), "header")
            } else {
                let user_id = body
                    .get("metadata")
                    .and_then(|meta| meta.get("user_id"))
                    .and_then(Value::as_str);
                let from_user_id = user_id.and_then(|raw| {
                    if let Ok(value) = serde_json::from_str::<Value>(raw) {
                        return value
                            .get("session_id")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                    }
                    raw.split("_session_").nth(1).map(str::to_string)
                });
                if let Some(id) = from_user_id {
                    (Some(id), "anthropic_user_id")
                } else {
                    let id = body
                        .get("metadata")
                        .and_then(|meta| meta.get("session_id"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    match id {
                        Some(id) => (Some(id), "metadata"),
                        None => (None, "none"),
                    }
                }
            }
        }
        Protocol::Chat => {
            let from_header = header_value(
                headers,
                &["session_id", "x-session-id", "session-id", "thread-id"],
            );
            if let Some(id) = from_header {
                (Some(id), "header")
            } else {
                let id = body
                    .get("metadata")
                    .and_then(|meta| meta.get("session_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                match id {
                    Some(id) => (Some(id), "metadata"),
                    None => (None, "none"),
                }
            }
        }
    };

    // 辅助请求：Codex 标题生成、Claude suggestion / CLAUDE.md / web search 等。
    if protocol == Protocol::Messages {
        let message_count = history_len(protocol, body).unwrap_or(0);
        let first = user_text.as_deref().unwrap_or("");
        if message_count <= 3 && (first.is_empty() || is_auxiliary_prompt(first)) {
            request_kind = "auxiliary".to_string();
        }
    } else if protocol == Protocol::Chat {
        if let Some(first) = user_text.as_deref() {
            if is_auxiliary_prompt(first) {
                request_kind = "auxiliary".to_string();
            }
        }
    }

    let client = protocol.client();
    let (session_key, final_source) = match client_session_id.as_deref() {
        Some(id) => (
            // 显式会话 id：同会话可跨 API Key / 模型，不把 key/model 放进命名空间。
            Some(format!("s:{}", sha256_hex(&format!("{client}|{id}")))),
            source.to_string(),
        ),
        None => match user_text.as_deref() {
            Some(text) => (
                // 无显式 id：用「协议 + API Key + 首条真实用户文本」做前缀指纹。
                Some(format!(
                    "fp:{}",
                    sha256_hex(&format!(
                        "{client}|{api_key_id}|{}",
                        text.chars().take(2048).collect::<String>()
                    ))
                )),
                "body_prefix".to_string(),
            ),
            None => (None, "none".to_string()),
        },
    };
    let has_key = session_key.is_some();

    SessionMeta {
        session_key,
        client_session_id,
        session_source: if has_key {
            final_source
        } else {
            "none".to_string()
        },
        session_client: Some(client.to_string()),
        session_title: session_title(user_text.as_deref()),
        request_kind,
        history_len: history_len(protocol, body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn codex_uses_prompt_cache_key_and_filters_environment() {
        let body = json!({
            "prompt_cache_key": "01a1ee2f-300d-4c1b-9b04-b1214eec6ec3",
            "client_metadata": {
                "session_id": "01a1ee2f-300d-4c1b-9b04-b1214eec6ec3",
                "thread_id": "01a1ee2f-300d-4c1b-9b04-b1214eec6ec3",
                "x-codex-turn-metadata": "{\"request_kind\":\"turn\",\"turn_trigger\":\"composer\"}"
            },
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "developer"}]},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "<environment_context>\n  <cwd>/tmp</cwd>\n</environment_context>\n\n你好，帮我看看项目"}
                ]}
            ]
        });
        let meta = extract(&HeaderMap::new(), &body, Protocol::Responses, "key");
        assert_eq!(meta.session_source, "prompt_cache_key");
        assert_eq!(
            meta.client_session_id.as_deref(),
            Some("01a1ee2f-300d-4c1b-9b04-b1214eec6ec3")
        );
        assert_eq!(meta.session_title.as_deref(), Some("你好，帮我看看项目"));
        assert_eq!(meta.request_kind, "turn");
        assert_eq!(meta.history_len, Some(2));
        assert!(meta.session_key.as_deref().unwrap().starts_with("s:"));
    }

    #[test]
    fn codex_title_generation_is_auxiliary() {
        let body = json!({
            "prompt_cache_key": "01a1ee2f-300d-4c1b-9b04-b1214eec6ec3",
            "client_metadata": {
                "session_id": "01a1ee2f-300d-4c1b-9b04-b1214eec6ec3",
                "x-codex-turn-metadata": "{\"request_kind\":\"turn\",\"turn_trigger\":\"thread_title\"}"
            },
            "input": "Generate a title for this conversation"
        });
        let meta = extract(&HeaderMap::new(), &body, Protocol::Responses, "key");
        assert_eq!(meta.request_kind, "auxiliary");
    }

    #[test]
    fn claude_reads_session_id_from_user_id_json_string() {
        let body = json!({
            "metadata": {
                "user_id": "{\"device_id\":\"dev\",\"account_uuid\":\"\",\"session_id\":\"9394ceeb-6736-4703-8754-82f0ee0b\"}"
            },
            "system": [{"type": "text", "text": "x-anthropic-billing-header"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "<system-reminder>ignore</system-reminder>上海天气"}]}
            ]
        });
        let meta = extract(&HeaderMap::new(), &body, Protocol::Messages, "key");
        assert_eq!(meta.session_source, "anthropic_user_id");
        assert_eq!(meta.session_title.as_deref(), Some("上海天气"));
        assert_eq!(meta.request_kind, "turn");
    }

    #[test]
    fn claude_header_takes_priority() {
        let body = json!({
            "metadata": {"user_id": "{\"session_id\":\"from-body\"}"},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let meta = extract(
            &headers(&[("x-claude-code-session-id", "from-header")]),
            &body,
            Protocol::Messages,
            "key",
        );
        assert_eq!(meta.session_source, "header");
        assert_eq!(meta.client_session_id.as_deref(), Some("from-header"));
    }

    #[test]
    fn chat_falls_back_to_prefix_hash() {
        let body = json!({
            "messages": [
                {"role": "system", "content": "system"},
                {"role": "user", "content": "帮我写一个函数"}
            ]
        });
        let a = extract(&HeaderMap::new(), &body, Protocol::Chat, "key-a");
        let b = extract(&HeaderMap::new(), &body, Protocol::Chat, "key-a");
        let c = extract(&HeaderMap::new(), &body, Protocol::Chat, "key-b");
        assert_eq!(a.session_source, "body_prefix");
        assert_eq!(a.session_key, b.session_key);
        assert_ne!(a.session_key, c.session_key);
        assert_eq!(a.session_title.as_deref(), Some("帮我写一个函数"));
    }

    #[test]
    fn session_title_is_none_for_auxiliary_prompts() {
        let body = json!({
            "messages": [{"role": "user", "content": "[SUGGESTION MODE: Suggest what the user might type next]"}]
        });
        let meta = extract(&HeaderMap::new(), &body, Protocol::Chat, "key");
        assert_eq!(meta.request_kind, "auxiliary");
        assert!(meta.session_title.is_none());
    }
}
