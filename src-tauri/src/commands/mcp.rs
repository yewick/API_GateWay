use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct SendMcpRequestInput {
    pub host: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct SendMcpRequestResult {
    pub status: u16,
    pub body: serde_json::Value,
}

/// MCP 测试台：向本机 MCP Server 发送一次 JSON-RPC 请求（initialize / tools/list / tools/call）。
/// 与 send_test_request 相同，直接走 reqwest 结构化 HTTP，绝不经过 shell，天然防注入。
#[tauri::command]
pub async fn send_mcp_request(
    input: SendMcpRequestInput,
) -> Result<SendMcpRequestResult, String> {
    let host = input.host.trim().to_string();
    if !(host.starts_with("http://") || host.starts_with("https://")) {
        return Err("地址必须以 http:// 或 https:// 开头".to_string());
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("地址包含非法空白或控制字符".to_string());
    }

    let url = format!("{}/mcp", host.trim_end_matches('/'));
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": input.method,
        "params": input.params.unwrap_or(serde_json::json!({})),
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("构造 HTTP 客户端失败: {}", e))?;
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let body = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or(serde_json::Value::String(text));

    Ok(SendMcpRequestResult { status, body })
}
