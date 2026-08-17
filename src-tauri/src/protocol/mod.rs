pub mod anthropic;
pub mod responses;

use axum::http::HeaderMap;
use serde_json::{json, Value};

/// Extract API key from either `Authorization: Bearer xxx` or `x-api-key: xxx` header.
pub fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    // Try Authorization: Bearer xxx first
    if let Some(auth) = headers.get("authorization").and_then(|h| h.to_str().ok()) {
        if let Some(key) = auth.strip_prefix("Bearer ") {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    // Fall back to x-api-key
    if let Some(key) = headers.get("x-api-key").and_then(|h| h.to_str().ok()) {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Extract a trace id from `x-trace-id` (or the legacy `Wali-Trace-Id`) header.
pub fn extract_trace_id(headers: &HeaderMap) -> Option<String> {
    for key in ["x-trace-id", "wali-trace-id"] {
        if let Some(v) = headers.get(key).and_then(|h| h.to_str().ok()) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Detect if a request is in Anthropic format by checking headers.
pub fn is_anthropic_request(headers: &HeaderMap, body: &Value) -> bool {
    if headers.contains_key("anthropic-version") {
        return true;
    }
    if headers.contains_key("x-api-key") && !headers.contains_key("authorization") {
        return true;
    }
    let _ = body;
    false
}

/// Detect if a request targets the Responses API format.
pub fn is_responses_request(body: &Value) -> bool {
    body.get("input").is_some() && body.get("messages").is_none()
}

/// Parse `usage` from an OpenAI SSE chunk (prompt/completion/total tokens).
/// Returns `Some` only when at least one token count is positive.
pub fn parse_usage_from_sse_chunk(text: &str) -> Option<(i64, i64, i64)> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") {
            continue;
        }
        let data_str = trimmed.trim_start_matches("data:").trim();
        if data_str == "[DONE]" || data_str.is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<Value>(data_str) {
            if let Some(usage) = json.get("usage") {
                let prompt = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let completion = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let total = usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                if total > 0 || prompt > 0 || completion > 0 {
                    return Some((prompt, completion, total));
                }
            }
        }
    }
    None
}

/// Map an OpenAI `finish_reason` to an Anthropic `stop_reason`.
pub(crate) fn finish_reason_to_stop_reason(finish_reason: &str) -> &str {
    match finish_reason {
        "stop" => "end_turn",
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        _ => "end_turn",
    }
}

// ---------------------------------------------------------------------------
// Anthropic → OpenAI
// ---------------------------------------------------------------------------

/// Convert an Anthropic Messages API request body to OpenAI Chat Completions format.
pub fn anthropic_to_openai(body: &Value) -> Value {
    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let messages = body.get("messages").cloned().unwrap_or(Value::Array(vec![]));
    let max_tokens = body.get("max_tokens").and_then(|m| m.as_u64()).unwrap_or(4096);
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    // Extract top-level system (string, or array of text blocks) and prepend it.
    let system = body.get("system").and_then(|s| match s {
        Value::String(str_val) => Some(str_val.clone()),
        Value::Array(arr) => {
            let texts: Vec<String> = arr
                .iter()
                .filter_map(|block| block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
                .collect();
            Some(texts.join(""))
        }
        _ => None,
    });

    let openai_messages = convert_anthropic_messages_to_openai(&messages, system);

    let mut openai_body = json!({
        "model": model,
        "messages": openai_messages,
        "max_tokens": max_tokens,
        "stream": stream,
    });

    if let Some(temp) = body.get("temperature") {
        openai_body["temperature"] = temp.clone();
    }
    if let Some(top_p) = body.get("top_p") {
        openai_body["top_p"] = top_p.clone();
    }
    if let Some(stop) = body.get("stop_sequences").cloned() {
        if stop.is_array() && !stop.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            openai_body["stop"] = stop;
        }
    }

    // tools: Anthropic {name, description, input_schema} → OpenAI {type:"function", function:{...}}
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let openai_tools: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name").and_then(|n| n.as_str())?;
                let mut func = json!({ "name": name });
                if let Some(desc) = tool.get("description").and_then(|d| d.as_str()) {
                    func["description"] = json!(desc);
                }
                if let Some(schema) = tool.get("input_schema").cloned() {
                    func["parameters"] = schema;
                }
                Some(json!({ "type": "function", "function": func }))
            })
            .collect();
        if !openai_tools.is_empty() {
            openai_body["tools"] = json!(openai_tools);
        }
    }

    // tool_choice: "auto"→"auto", "any"→"required", {type:"tool",name}→{type:"function",function:{name}}
    if let Some(tc) = body.get("tool_choice").cloned() {
        if let Some(choice) = convert_anthropic_tool_choice(&tc) {
            openai_body["tool_choice"] = choice;
        }
    }

    openai_body
}

fn convert_anthropic_tool_choice(tc: &Value) -> Option<Value> {
    match tc {
        Value::String(s) => match s.as_str() {
            "auto" => Some(json!("auto")),
            "any" => Some(json!("required")),
            "none" => Some(json!("none")),
            _ => None,
        },
        Value::Object(_) => {
            let ty = tc.get("type").and_then(|t| t.as_str())?;
            match ty {
                "auto" => Some(json!("auto")),
                "any" => Some(json!("required")),
                "none" => Some(json!("none")),
                "tool" => {
                    let name = tc.get("name").and_then(|n| n.as_str())?;
                    Some(json!({ "type": "function", "function": { "name": name } }))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn convert_anthropic_messages_to_openai(messages: &Value, system: Option<String>) -> Value {
    let mut msgs: Vec<Value> = Vec::new();

    // Prepend system message if present.
    if let Some(sys) = system {
        msgs.push(json!({ "role": "system", "content": sys }));
    }

    let arr = match messages.as_array() {
        Some(a) => a,
        None => return Value::Array(msgs),
    };

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();
        let content = msg.get("content").cloned().unwrap_or(Value::Null);

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut tool_results: Vec<Value> = Vec::new();

        if let Some(blocks) = content.as_array() {
            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(t.to_string());
                        }
                    }
                    "thinking" => {
                        if let Some(t) = block.get("thinking").and_then(|t| t.as_str()) {
                            text_parts.push(t.to_string());
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").cloned().unwrap_or(Value::Object(serde_json::Map::new()));
                        // Anthropic `input` is a JSON object; OpenAI `arguments` is a JSON string.
                        let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": arguments }
                        }));
                    }
                    "tool_result" => {
                        let tool_call_id = block.get("tool_use_id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        let result_content = block.get("content").cloned().unwrap_or(Value::String(String::new()));
                        let text = extract_block_text(&result_content);
                        tool_results.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": text
                        }));
                    }
                    "image" => { /* skipped — future conversion */ }
                    _ => {}
                }
            }
        } else if let Some(t) = content.as_str() {
            text_parts.push(t.to_string());
        }

        if !tool_calls.is_empty() {
            let mut m = json!({ "role": "assistant", "content": text_parts.join("") });
            m["tool_calls"] = json!(tool_calls);
            msgs.push(m);
        } else if !tool_results.is_empty() {
            for tr in tool_results {
                msgs.push(tr);
            }
            if !text_parts.is_empty() {
                msgs.push(json!({ "role": role, "content": text_parts.join("") }));
            }
        } else {
            msgs.push(json!({ "role": role, "content": text_parts.join("") }));
        }
    }

    Value::Array(msgs)
}

/// Extract plain text from a content value that may be a string or an array of `{text}` blocks.
fn extract_block_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => content.to_string(),
    }
}

// ---------------------------------------------------------------------------
// OpenAI → Anthropic
// ---------------------------------------------------------------------------

/// Convert an OpenAI Chat Completions response to Anthropic Messages format.
pub fn openai_to_anthropic(openai_resp: &Value, model: &str) -> Value {
    // Upstream error → Anthropic error shape.
    if openai_resp.get("error").is_some() {
        return openai_error_to_anthropic(openai_resp);
    }

    let choice = openai_resp.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
    let message = choice.and_then(|c| c.get("message"));

    let content_text = message.and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("").to_string();
    let finish_reason = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .unwrap_or("stop");

    let stop_reason = finish_reason_to_stop_reason(finish_reason);

    let mut content_blocks: Vec<Value> = Vec::new();
    if !content_text.is_empty() {
        content_blocks.push(json!({ "type": "text", "text": content_text }));
    }

    // OpenAI tool_calls → Anthropic tool_use blocks.
    if let Some(tool_calls) = message.and_then(|m| m.get("tool_calls")).and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let func = tc.get("function").cloned().unwrap_or(Value::Null);
            let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("");
            // arguments (JSON string) → input (JSON object)
            let input: Value = serde_json::from_str(args_str).unwrap_or(Value::Null);
            content_blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }));
        }
    }

    let prompt_tokens = openai_resp
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let completion_tokens = openai_resp
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    let id = openai_resp
        .get("id")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()));

    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content_blocks,
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": prompt_tokens,
            "output_tokens": completion_tokens
        }
    })
}

fn openai_error_to_anthropic(openai_resp: &Value) -> Value {
    let err = openai_resp.get("error").cloned().unwrap_or(Value::Null);
    let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error").to_string();
    let typ = err.get("type").and_then(|t| t.as_str()).unwrap_or("api_error").to_string();
    json!({
        "type": "error",
        "error": { "type": typ, "message": message }
    })
}

// ---------------------------------------------------------------------------
// Responses → OpenAI
// ---------------------------------------------------------------------------

/// Convert an OpenAI Responses API request body to Chat Completions format.
pub fn responses_to_openai(body: &Value) -> Value {
    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();

    let mut openai_messages = if let Some(input) = body.get("input") {
        convert_responses_input_to_messages(input)
    } else {
        Value::Array(vec![])
    };

    // instructions → system message (prepend).
    if let Some(instructions) = body.get("instructions").and_then(|i| i.as_str()) {
        if let Some(arr) = openai_messages.as_array_mut() {
            arr.insert(0, json!({ "role": "system", "content": instructions }));
        }
    }

    let max_tokens = body.get("max_output_tokens").and_then(|m| m.as_u64()).unwrap_or(4096);
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    let mut openai_body = json!({
        "model": model,
        "messages": openai_messages,
        "max_tokens": max_tokens,
        "stream": stream,
    });

    if let Some(temp) = body.get("temperature") {
        openai_body["temperature"] = temp.clone();
    }
    if let Some(top_p) = body.get("top_p") {
        openai_body["top_p"] = top_p.clone();
    }

    // tools: Responses flat {type:"function",name,parameters,description} → OpenAI nested.
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let openai_tools: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                let ty = tool.get("type").and_then(|t| t.as_str())?;
                if ty != "function" {
                    return None; // skip built-in tools (web_search / file_search / computer_use)
                }
                let name = tool.get("name").and_then(|n| n.as_str())?;
                let mut func = json!({ "name": name });
                if let Some(desc) = tool.get("description").and_then(|d| d.as_str()) {
                    func["description"] = json!(desc);
                }
                if let Some(params) = tool.get("parameters").cloned() {
                    func["parameters"] = params;
                }
                Some(json!({ "type": "function", "function": func }))
            })
            .collect();
        if !openai_tools.is_empty() {
            openai_body["tools"] = json!(openai_tools);
        }
    }

    if let Some(tc) = body.get("tool_choice").cloned() {
        openai_body["tool_choice"] = tc;
    }

    openai_body
}

fn convert_responses_input_to_messages(input: &Value) -> Value {
    let mut msgs: Vec<Value> = Vec::new();
    let arr = match input.as_array() {
        Some(a) => a,
        None => return Value::Array(msgs),
    };

    for item in arr {
        let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "message" => {
                let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();
                let content = item.get("content").cloned().unwrap_or(Value::String(String::new()));
                let text = extract_responses_text(&content);
                msgs.push(json!({ "role": role, "content": text }));
            }
            "function_call" => {
                let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let arguments = item.get("arguments").cloned().unwrap_or(Value::String(String::new()));
                let args_str = if arguments.is_string() {
                    arguments.as_str().unwrap_or("").to_string()
                } else {
                    serde_json::to_string(&arguments).unwrap_or_else(|_| "{}".to_string())
                };
                msgs.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": { "name": name, "arguments": args_str }
                    }]
                }));
            }
            "function_call_output" => {
                let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                let output = item.get("output").cloned().unwrap_or(Value::String(String::new()));
                let text = extract_responses_text(&output);
                msgs.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": text
                }));
            }
            _ => {}
        }
    }

    Value::Array(msgs)
}

fn extract_responses_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// OpenAI → Responses
// ---------------------------------------------------------------------------

/// Convert an OpenAI Chat Completions response to Responses API format.
pub fn openai_to_responses(openai_resp: &Value, model: &str) -> Value {
    // Responses API uses the same OpenAI-style error shape; pass through.
    if openai_resp.get("error").is_some() {
        return openai_resp.clone();
    }

    let choice = openai_resp.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
    let message = choice.and_then(|c| c.get("message"));
    let content = message.and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("").to_string();
    let finish_reason = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .unwrap_or("stop")
        .to_string();

    let mut output: Vec<Value> = Vec::new();

    // tool_calls → function_call output items.
    if let Some(tool_calls) = message.and_then(|m| m.get("tool_calls")).and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let func = tc.get("function").cloned().unwrap_or(Value::Null);
            let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let arguments = func.get("arguments").cloned().unwrap_or(Value::String(String::new()));
            output.push(json!({
                "id": format!("fc_{}", uuid::Uuid::new_v4().simple()),
                "type": "function_call",
                "call_id": call_id,
                "name": name,
                "arguments": arguments,
                "status": "completed"
            }));
        }
    }

    // Text content → message output item.
    if !content.is_empty() || output.is_empty() {
        output.push(json!({
            "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": content }],
            "status": "completed"
        }));
    }

    let prompt_tokens = openai_resp
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let completion_tokens = openai_resp
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    let id = openai_resp
        .get("id")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("resp_{}", uuid::Uuid::new_v4().simple()));

    json!({
        "id": id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": "completed",
        "model": model,
        "output": output,
        "usage": {
            "input_tokens": prompt_tokens,
            "output_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        },
        "finish_reason": finish_reason
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_api_key_prefers_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-bearer".parse().unwrap());
        headers.insert("x-api-key", "sk-xkey".parse().unwrap());
        assert_eq!(extract_api_key(&headers), Some("sk-bearer".to_string()));
    }

    #[test]
    fn test_extract_api_key_falls_back_to_x_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "sk-xkey".parse().unwrap());
        assert_eq!(extract_api_key(&headers), Some("sk-xkey".to_string()));
    }

    #[test]
    fn test_anthropic_to_openai_system_and_text() {
        let body = json!({
            "model": "claude-sonnet-4",
            "max_tokens": 100,
            "system": "You are helpful",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hello"}]}
            ]
        });
        let out = anthropic_to_openai(&body);
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "hello");
        assert_eq!(out["max_tokens"], 100);
    }

    #[test]
    fn test_anthropic_to_openai_tool_use() {
        let body = json!({
            "model": "claude-sonnet-4",
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Searching"},
                    {"type": "tool_use", "id": "toolu_1", "name": "search", "input": {"q": "x"}}
                ]}
            ]
        });
        let out = anthropic_to_openai(&body);
        let msgs = out["messages"].as_array().unwrap();
        let tool_calls = msgs[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "search");
        assert_eq!(tool_calls[0]["function"]["arguments"], r#"{"q":"x"}"#);
    }

    #[test]
    fn test_openai_to_anthropic_text() {
        let resp = json!({
            "id": "chatcmpl-1",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7}
        });
        let out = openai_to_anthropic(&resp, "claude-sonnet-4");
        assert_eq!(out["type"], "message");
        assert_eq!(out["stop_reason"], "end_turn");
        assert_eq!(out["content"][0]["text"], "hi");
        assert_eq!(out["usage"]["input_tokens"], 5);
    }

    #[test]
    fn test_openai_to_anthropic_error() {
        let resp = json!({"error": {"message": "bad key", "type": "invalid_request_error"}});
        let out = openai_to_anthropic(&resp, "m");
        assert_eq!(out["type"], "error");
        assert_eq!(out["error"]["message"], "bad key");
    }

    #[test]
    fn test_responses_to_openai_instructions_and_input() {
        let body = json!({
            "model": "gpt-5",
            "instructions": "Be concise",
            "max_output_tokens": 50,
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}
            ]
        });
        let out = responses_to_openai(&body);
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "Be concise");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "hi");
        assert_eq!(out["max_tokens"], 50);
    }

    #[test]
    fn test_openai_to_responses_text() {
        let resp = json!({
            "id": "chatcmpl-1",
            "choices": [{"message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });
        let out = openai_to_responses(&resp, "gpt-5");
        assert_eq!(out["object"], "response");
        assert_eq!(out["output"][0]["type"], "message");
        assert_eq!(out["output"][0]["content"][0]["text"], "hello");
    }
}
