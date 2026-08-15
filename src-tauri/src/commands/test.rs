use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SendTestRequestInput {
    pub host: String,
    pub api_key: String,
    pub model: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SendTestRequestResult {
    pub status: u16,
    pub body: serde_json::Value,
}

/// 测试台：向网关发送一次 chat/completions 请求。
/// 注意：直接走 reqwest 结构化 HTTP，绝不经过 shell，天然防注入。
#[tauri::command]
pub async fn send_test_request(
    input: SendTestRequestInput,
) -> Result<SendTestRequestResult, String> {
    let host = input.host.trim().to_string();
    if !(host.starts_with("http://") || host.starts_with("https://")) {
        return Err("地址必须以 http:// 或 https:// 开头".to_string());
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("地址包含非法空白或控制字符".to_string());
    }

    let url = format!("{}/v1/chat/completions", host.trim_end_matches('/'));
    let payload = serde_json::json!({
        "model": input.model,
        "messages": [{ "role": "user", "content": input.content }],
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", input.api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let body = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or(serde_json::Value::String(text));

    Ok(SendTestRequestResult { status, body })
}
