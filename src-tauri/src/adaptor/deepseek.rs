use async_trait::async_trait;
use super::*;

pub struct DeepSeekAdaptor;

#[async_trait]
impl Adaptor for DeepSeekAdaptor {
    fn channel_type(&self) -> &'static str { "deepseek" }
    fn default_models(&self) -> Vec<&'static str> {
        vec!["deepseek-v4-flash", "deepseek-v4-pro"]
    }
    fn default_base_url(&self) -> &str { "https://api.deepseek.com/v1" }

    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        let start = std::time::Instant::now();
        // 用 GET /models 接口测连通性，成本最低（该接口只读，支持 GET）
        let url = format!("{}/models", config.base_url.trim_end_matches('/'));

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match resp {
            Ok(r) => {
                let latency = start.elapsed().as_millis() as u64;
                if r.status().is_success() {
                    Ok(TestResult { success: true, message: "连接成功".to_string(), latency_ms: latency })
                } else {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    Ok(TestResult {
                        success: false,
                        message: format!("HTTP {}: {}", status, body.chars().take(200).collect::<String>()),
                        latency_ms: latency,
                    })
                }
            }
            Err(e) => Ok(TestResult {
                success: false,
                message: format!("连接失败: {}", e),
                latency_ms: start.elapsed().as_millis() as u64,
            }),
        }
    }

    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error> {
        let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
        let body = apply_reasoning_compat(super::openai::apply_model_mapping(
            &request.body,
            &config.model_mapping,
        ));
        let client = http_client(Some(120))?;
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let json: serde_json::Value = resp.json().await?;
        let usage = json.get("usage").map(|u| TokenUsage {
            prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            completion_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        });
        Ok((status, json, usage))
    }

    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
        let mut body = apply_reasoning_compat(super::openai::apply_model_mapping(
            &request.body,
            &config.model_mapping,
        ));
        body["stream"] = serde_json::Value::Bool(true);
        let client = http_client(None)?;
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        Ok(resp)
    }
}

/// DeepSeek thinking 模式兼容：
/// 历史中只要出现带 `tool_calls` 的 assistant 消息，就对本请求关闭 thinking。
///
/// 原因：thinking 模式要求把产生工具调用时的 `reasoning_content` 原样回传，但
/// Responses 客户端（Codex）不会回传 reasoning item；实测即便网关回填也无法通过
/// 上游校验。关闭 thinking 可以让工具循环稳定工作。
fn apply_reasoning_compat(mut body: serde_json::Value) -> serde_json::Value {
    let has_tool_calls = body
        .get("messages")
        .and_then(|messages| messages.as_array())
        .is_some_and(|messages| {
            messages.iter().any(|message| {
                message.get("role").and_then(|role| role.as_str()) == Some("assistant")
                    && message
                        .get("tool_calls")
                        .and_then(|tool_calls| tool_calls.as_array())
                        .is_some_and(|tool_calls| !tool_calls.is_empty())
            })
        });

    if has_tool_calls {
        body["thinking"] = serde_json::json!({ "type": "disabled" });
        if let Some(object) = body.as_object_mut() {
            object.remove("reasoning_effort");
        }
    }

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn disables_thinking_for_tool_history() {
        let call_id = format!("tool-{}", uuid::Uuid::new_v4().simple());
        let body = json!({
            "messages": [
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": call_id, "type": "function", "function": {"name": "f", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": call_id, "content": "x"}
            ],
            "reasoning_effort": "high"
        });
        let out = apply_reasoning_compat(body);
        assert_eq!(out["thinking"]["type"], "disabled");
        assert!(out.get("reasoning_effort").is_none());
    }

    #[test]
    fn leaves_plain_chat_unchanged() {
        let body = json!({"messages": [{"role": "user", "content": "hi"}], "reasoning_effort": "high"});
        let out = apply_reasoning_compat(body);
        assert!(out.get("thinking").is_none());
        assert_eq!(out["reasoning_effort"], "high");
    }
}
