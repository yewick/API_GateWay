//! OpenAI Chat Completions 流式响应聚合器。
//!
//! 仅用于日志旁路：把 `delta` 片段拼成完整的 `chat.completion` 对象，
//! 不改变实际转发给客户端的字节。

use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
struct ToolState {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ChoiceState {
    content: String,
    reasoning: String,
    tool_calls: BTreeMap<usize, ToolState>,
    finish_reason: Option<String>,
}

pub struct ChatStreamAggregator {
    model: String,
    response_id: String,
    buffer: Vec<u8>,
    choices: BTreeMap<usize, ChoiceState>,
    usage: Option<(i64, i64, i64)>,
    done: bool,
}

impl ChatStreamAggregator {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            response_id: format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()),
            buffer: Vec::new(),
            choices: BTreeMap::new(),
            usage: None,
            done: false,
        }
    }

    /// 喂入一段上游字节；返回是否已经看到 `data: [DONE]`。
    pub fn push(&mut self, bytes: &[u8]) -> bool {
        self.buffer.extend_from_slice(bytes);
        let mut done = false;
        loop {
            let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') else {
                break;
            };
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim_end();
            let Some(rest) = line.strip_prefix("data:") else {
                continue;
            };
            let data = rest.trim();
            if data == "[DONE]" {
                self.done = true;
                done = true;
                continue;
            }
            if data.is_empty() {
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                self.handle_chunk(&chunk);
            }
        }
        done
    }

    pub fn usage(&self) -> Option<(i64, i64, i64)> {
        self.usage
    }

    fn handle_chunk(&mut self, chunk: &Value) {
        if let Some(usage) = chunk.get("usage") {
            let prompt = usage
                .get("prompt_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let completion = usage
                .get("completion_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let total = usage
                .get("total_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(prompt + completion);
            if total > 0 || prompt > 0 || completion > 0 {
                self.usage = Some((prompt, completion, total));
            }
        }

        let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
            return;
        };
        for choice in choices {
            let index = choice
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let state = self.choices.entry(index).or_default();
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content").and_then(Value::as_str) {
                    state.content.push_str(content);
                }
                if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
                    state.reasoning.push_str(reasoning);
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for tool_call in tool_calls {
                        let tool_index = tool_call
                            .get("index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as usize;
                        let tool = state.tool_calls.entry(tool_index).or_default();
                        if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                            if !id.is_empty() {
                                tool.id = id.to_string();
                            }
                        }
                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                if !name.is_empty() {
                                    tool.name.push_str(name);
                                }
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                tool.arguments.push_str(arguments);
                            }
                        }
                    }
                }
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                if !reason.is_empty() && reason != "null" {
                    state.finish_reason = Some(reason.to_string());
                }
            }
        }
    }

    /// 构造最终 `chat.completion` 对象；没有内容时返回 None。
    pub fn final_response(&self) -> Option<Value> {
        if self.choices.is_empty() {
            return None;
        }
        let choices: Vec<Value> = self
            .choices
            .iter()
            .map(|(index, state)| {
                let mut message = json!({
                    "role": "assistant",
                    "content": state.content
                });
                if !state.reasoning.is_empty() {
                    message["reasoning_content"] = json!(state.reasoning);
                }
                if !state.tool_calls.is_empty() {
                    let tools: Vec<Value> = state
                        .tool_calls
                        .values()
                        .map(|tool| {
                            json!({
                                "id": tool.id,
                                "type": "function",
                                "function": {
                                    "name": tool.name,
                                    "arguments": tool.arguments
                                }
                            })
                        })
                        .collect();
                    message["tool_calls"] = Value::Array(tools);
                }
                json!({
                    "index": index,
                    "message": message,
                    "finish_reason": state.finish_reason
                })
            })
            .collect();

        let mut value = json!({
            "id": self.response_id,
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": self.model,
            "choices": choices
        });
        if let Some((prompt, completion, total)) = self.usage {
            value["usage"] = json!({
                "prompt_tokens": prompt,
                "completion_tokens": completion,
                "total_tokens": total
            });
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_text_and_usage() {
        let mut agg = ChatStreamAggregator::new("m");
        assert!(!agg.push(
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n"
        ));
        agg.push(
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n",
        );
        assert!(agg.push(b"data: [DONE]\n"));
        let body = agg.final_response().unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "Hello world");
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert_eq!(body["usage"]["total_tokens"], 5);
        assert_eq!(agg.usage(), Some((3, 2, 5)));
    }

    #[test]
    fn aggregates_tool_calls() {
        let mut agg = ChatStreamAggregator::new("m");
        agg.push(
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"search\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n",
        );
        agg.push(
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"q\\\":\\\"x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n",
        );
        let body = agg.final_response().unwrap();
        let tool = &body["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tool["id"], "call_1");
        assert_eq!(tool["function"]["name"], "search");
        assert_eq!(tool["function"]["arguments"], "{\"q\":\"x\"}");
    }

    #[test]
    fn handles_done_across_chunks() {
        let mut agg = ChatStreamAggregator::new("m");
        assert!(!agg.push(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n"));
        assert!(!agg.push(b"data: [DO"));
        assert!(agg.push(b"NE]\n"));
    }
}
