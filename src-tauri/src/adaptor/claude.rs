use async_trait::async_trait;
use super::*;

pub struct ClaudeAdaptor;

#[async_trait]
impl Adaptor for ClaudeAdaptor {
    fn channel_type(&self) -> &'static str { "claude" }
    fn default_models(&self) -> Vec<&'static str> {
        vec!["claude-sonnet-4-20250514", "claude-3-5-haiku-20241022"]
    }
    fn default_base_url(&self) -> &str { "https://api.anthropic.com" }

    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        let start = std::time::Instant::now();
        let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));
        // Claude 没有廉价的 GET /models 鉴权接口，发一个 max_tokens=1 的最小请求
        let body = serde_json::json!({
            "model": config.models.first().map(|s| s.as_str()).unwrap_or("claude-3-5-haiku-20241022"),
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}]
        });
        let client = reqwest::Client::new();
        let result = client.post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;
        let latency = start.elapsed().as_millis() as u64;
        match result {
            Ok(r) => {
                // 400 也算连通（说明认证已通过，只是请求参数问题）
                if r.status().is_success() || r.status().as_u16() == 400 {
                    Ok(TestResult { success: true, message: "连接成功".to_string(), latency_ms: latency })
                } else {
                    Ok(TestResult { success: false, message: format!("HTTP {}", r.status()), latency_ms: latency })
                }
            }
            Err(e) => Ok(TestResult { success: false, message: format!("连接失败: {}", e), latency_ms: latency }),
        }
    }

    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error> {
        let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));
        let (model, mut claude_body) = build_claude_request(&request.body);
        claude_body["stream"] = serde_json::Value::Bool(request.stream);

        let client = reqwest::Client::new();
        let resp = client.post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&claude_body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        let claude_json: serde_json::Value = resp.json().await?;

        // Claude 响应 → OpenAI 响应
        let openai_response = convert_claude_to_openai(&claude_json, &model);
        let usage = openai_response.get("usage").and_then(|u| {
            Some(TokenUsage {
                prompt_tokens: u.get("prompt_tokens")?.as_u64()?,
                completion_tokens: u.get("completion_tokens")?.as_u64()?,
                total_tokens: u.get("total_tokens")?.as_u64()?,
            })
        });

        Ok((status, openai_response, usage))
    }

    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));
        let (_model, mut claude_body) = build_claude_request(&request.body);
        claude_body["stream"] = serde_json::Value::Bool(true);

        let client = reqwest::Client::new();
        let resp = client.post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&claude_body)
            .send()
            .await?;

        Ok(resp)
    }
}

/// 将 OpenAI 格式的请求体转换为 Claude Messages API 格式。
/// 返回 (model, claude_body)；stream 由调用方按需设置。
fn build_claude_request(openai_body: &serde_json::Value) -> (String, serde_json::Value) {
    let model = openai_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("claude-3-5-haiku-20241022")
        .to_string();
    let messages = openai_body
        .get("messages")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let max_tokens = openai_body
        .get("max_tokens")
        .and_then(|m| m.as_u64())
        .unwrap_or(4096);
    let temperature = openai_body.get("temperature").cloned();

    let (system, claude_messages) = convert_openai_messages_to_claude(&messages);

    let mut claude_body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": claude_messages,
    });
    if let Some(sys) = system {
        claude_body["system"] = serde_json::Value::String(sys);
    }
    if let Some(temp) = temperature {
        claude_body["temperature"] = temp;
    }
    (model, claude_body)
}

/// 请求转换（OpenAI → Claude）
/// OpenAI 的 system 是 messages 数组第一条，Claude 的 system 是独立顶层字段；
/// Claude 的 messages 只允许 user / assistant 两种角色。
fn convert_openai_messages_to_claude(messages: &serde_json::Value) -> (Option<String>, serde_json::Value) {
    let msgs = match messages.as_array() {
        Some(arr) => arr,
        None => return (None, serde_json::Value::Array(vec![])),
    };

    let mut system = None;
    let mut claude_msgs = Vec::new();

    for msg in msgs {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = msg.get("content").cloned().unwrap_or(serde_json::Value::String(String::new()));

        if role == "system" {
            // system 消息提取为顶层字段
            if let Some(s) = content.as_str() {
                system = Some(s.to_string());
            }
        } else {
            // assistant 保留，其他一律视为 user
            claude_msgs.push(serde_json::json!({
                "role": if role == "assistant" { "assistant" } else { "user" },
                "content": content,
            }));
        }
    }

    (system, serde_json::Value::Array(claude_msgs))
}

/// 响应转换（Claude → OpenAI）
fn convert_claude_to_openai(claude_json: &serde_json::Value, model: &str) -> serde_json::Value {
    // content 是 block 数组，提取所有 text block 拼接
    let content = claude_json
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    // input_tokens/output_tokens → prompt_tokens/completion_tokens
    let prompt_tokens = claude_json
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let completion_tokens = claude_json
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    serde_json::json!({
        "id": claude_json.get("id").cloned()
            .unwrap_or(serde_json::Value::String("chatcmpl-converted".to_string())),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens,
        }
    })
}
