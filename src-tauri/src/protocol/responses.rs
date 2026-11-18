//! Responses SSE 流式转换：把 OpenAI Chat Completions 的 SSE 增量转换为 Responses
//! 的标准事件序列。
//!
//! 事件顺序：
//! `response.created` → `response.output_item.added` → `response.content_part.added`
//! → `response.output_text.delta` … → `response.output_text.done`
//! → `response.content_part.done` → `response.output_item.done` → `response.completed`。
//!
//! 工具调用同理：`output_item.added` → `function_call_arguments.delta` …
//! → `function_call_arguments.done` → `output_item.done`。
//!
//! 只负责「流式 SSE 互转」与 usage 解析；非流式 JSON 双向转换见 [`super`]。

use serde_json::{json, Value};
use std::collections::HashMap;

struct ToolCallState {
    item_id: String,
    call_id: String,
    output_index: usize,
    name: String,
    arguments: String,
}

/// 有状态转换器：逐块消费 OpenAI SSE 字节，输出 Responses SSE 事件字符串。
pub struct ResponsesStreamConverter {
    model: String,
    response_id: String,
    /// 未完成的行缓冲（跨 TCP 分片）。
    buffer: Vec<u8>,
    started: bool,
    finished: bool,
    /// 文本 item 是否已经通过 `output_item.added` 声明。
    text_started: bool,
    text: String,
    /// 当前文本输出的 item 标识。
    msg_item_id: String,
    msg_output_index: Option<usize>,
    /// 工具调用状态：index → state。
    tool_calls: HashMap<usize, ToolCallState>,
    next_output_index: usize,
    input_tokens: u64,
    output_tokens: u64,
}

impl ResponsesStreamConverter {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            response_id: format!("resp_{}", uuid::Uuid::new_v4().simple()),
            buffer: Vec::new(),
            started: false,
            finished: false,
            text_started: false,
            text: String::new(),
            msg_item_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            msg_output_index: None,
            tool_calls: HashMap::new(),
            next_output_index: 0,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') else {
                break;
            };
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim_end();
            if let Some(rest) = line.strip_prefix("data:") {
                let data = rest.trim();
                self.process_data_line(data, &mut out);
            }
        }
        out
    }

    pub fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }

        if !self.buffer.is_empty() {
            let line = String::from_utf8_lossy(&self.buffer).trim_end().to_string();
            self.buffer.clear();
            if let Some(rest) = line.strip_prefix("data:") {
                let data = rest.trim();
                if data != "[DONE]" {
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        self.handle_chunk(&json, &mut out);
                    }
                }
            }
        }

        self.complete("stop", &mut out);
        out
    }

    /// 上游流中断：发送 `response.failed` 终端事件（绝不发 `response.completed`）。
    /// 已流出的增量内容保持原样；这里不补发任何 `*.done`，避免把中断的
    /// 文本/工具参数伪装成正常完成。
    pub fn finish_error(&mut self, message: &str) -> Vec<String> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        // 丢弃不完整的残余行，避免误解析出 finish_reason 而走成功收尾。
        self.buffer.clear();

        let ev = json!({
            "type": "response.failed",
            "response": {
                "id": self.response_id.clone(),
                "object": "response",
                "status": "failed",
                "model": self.model.clone(),
                "output": [],
                "error": { "code": "server_error", "message": message }
            }
        });
        vec![self.event("response.failed", &ev)]
    }

    fn process_data_line(&mut self, data: &str, out: &mut Vec<String>) {
        if data == "[DONE]" || data.is_empty() {
            return;
        }
        if let Ok(json) = serde_json::from_str::<Value>(data) {
            self.handle_chunk(&json, out);
        }
    }

    fn handle_chunk(&mut self, chunk: &Value, out: &mut Vec<String>) {
        if let Some(usage) = chunk.get("usage") {
            if let Some(p) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                self.input_tokens = p;
            }
            if let Some(c) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                self.output_tokens = c;
            }
        }

        let choice = chunk
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first());
        let delta = choice.and_then(|c| c.get("delta"));
        let finish_reason = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|f| f.as_str());

        if !self.started {
            self.started = true;
            out.push(self.event(
                "response.created",
                &json!({
                    "type": "response.created",
                    "response": {
                        "id": self.response_id,
                        "object": "response",
                        "status": "in_progress",
                        "model": self.model,
                        "output": []
                    }
                }),
            ));
        }

        if let Some(delta) = delta {
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    self.ensure_text_started(out);
                    self.text.push_str(content);
                    let output_index = self.msg_output_index.unwrap_or(0);
                    out.push(self.event(
                        "response.output_text.delta",
                        &json!({
                            "type": "response.output_text.delta",
                            "item_id": self.msg_item_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "delta": content
                        }),
                    ));
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tool_call in tool_calls {
                    self.handle_tool_call(tool_call, out);
                }
            }
        }

        if let Some(finish_reason) = finish_reason {
            if !finish_reason.is_empty() && finish_reason != "null" {
                self.complete(finish_reason, out);
            }
        }
    }

    fn ensure_text_started(&mut self, out: &mut Vec<String>) {
        if self.text_started {
            return;
        }
        self.text_started = true;

        let output_index = self.next_output_index;
        self.next_output_index += 1;
        self.msg_output_index = Some(output_index);

        out.push(self.event(
            "response.output_item.added",
            &json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": {
                    "id": self.msg_item_id,
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "status": "in_progress"
                }
            }),
        ));
        out.push(self.event(
            "response.content_part.added",
            &json!({
                "type": "response.content_part.added",
                "item_id": self.msg_item_id,
                "output_index": output_index,
                "content_index": 0,
                "part": { "type": "output_text", "text": "" }
            }),
        ));
    }

    fn handle_tool_call(&mut self, tool_call: &Value, out: &mut Vec<String>) {
        let index = tool_call
            .get("index")
            .and_then(|i| i.as_u64())
            .unwrap_or(0) as usize;
        let arguments = tool_call
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|a| a.as_str())
            .unwrap_or("");

        if !self.tool_calls.contains_key(&index) {
            let call_id = tool_call
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let name = tool_call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let item_id = format!("fc_{}", uuid::Uuid::new_v4().simple());
            let output_index = self.next_output_index;
            self.next_output_index += 1;

            self.tool_calls.insert(
                index,
                ToolCallState {
                    item_id: item_id.clone(),
                    call_id: call_id.clone(),
                    output_index,
                    name: name.clone(),
                    arguments: String::new(),
                },
            );

            out.push(self.event(
                "response.output_item.added",
                &json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "function_call",
                        "call_id": call_id,
                        "name": name,
                        "arguments": "",
                        "status": "in_progress"
                    }
                }),
            ));
        }

        if !arguments.is_empty() {
            let Some(state) = self.tool_calls.get_mut(&index) else {
                return;
            };
            state.arguments.push_str(arguments);
            let item_id = state.item_id.clone();
            let output_index = state.output_index;
            out.push(self.event(
                "response.function_call_arguments.delta",
                &json!({
                    "type": "response.function_call_arguments.delta",
                    "item_id": item_id,
                    "output_index": output_index,
                    "delta": arguments
                }),
            ));
        }
    }

    fn complete(&mut self, finish_reason: &str, out: &mut Vec<String>) {
        if self.finished {
            return;
        }
        self.finished = true;

        let mut output_items: Vec<(usize, Value)> = Vec::new();

        if self.text_started {
            let output_index = self.msg_output_index.unwrap_or(0);
            let text = self.text.clone();

            out.push(self.event(
                "response.output_text.done",
                &json!({
                    "type": "response.output_text.done",
                    "item_id": self.msg_item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "text": text
                }),
            ));

            let part = json!({ "type": "output_text", "text": text });
            out.push(self.event(
                "response.content_part.done",
                &json!({
                    "type": "response.content_part.done",
                    "item_id": self.msg_item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "part": part
                }),
            ));

            let item = json!({
                "id": self.msg_item_id,
                "type": "message",
                "role": "assistant",
                "content": [part],
                "status": "completed"
            });
            out.push(self.event(
                "response.output_item.done",
                &json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": item
                }),
            ));
            output_items.push((output_index, item));
        }

        let mut tool_calls: Vec<(usize, &ToolCallState)> = self
            .tool_calls
            .values()
            .map(|state| (state.output_index, state))
            .collect();
        tool_calls.sort_by_key(|(output_index, _)| *output_index);

        for (output_index, state) in tool_calls {
            out.push(self.event(
                "response.function_call_arguments.done",
                &json!({
                    "type": "response.function_call_arguments.done",
                    "item_id": state.item_id,
                    "output_index": output_index,
                    "arguments": state.arguments
                }),
            ));

            let item = json!({
                "id": state.item_id,
                "type": "function_call",
                "call_id": state.call_id,
                "name": state.name,
                "arguments": state.arguments,
                "status": "completed"
            });
            out.push(self.event(
                "response.output_item.done",
                &json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": item
                }),
            ));
            output_items.push((output_index, item));
        }

        output_items.sort_by_key(|(output_index, _)| *output_index);
        let output: Vec<Value> = output_items
            .into_iter()
            .map(|(_, item)| item)
            .collect();

        out.push(self.event(
            "response.completed",
            &json!({
                "type": "response.completed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "status": "completed",
                    "model": self.model,
                    "output": output,
                    "finish_reason": finish_reason,
                    "usage": {
                        "input_tokens": self.input_tokens,
                        "output_tokens": self.output_tokens,
                        "total_tokens": self.input_tokens + self.output_tokens
                    }
                }
            }),
        ));
    }

    fn event(&self, name: &str, data: &Value) -> String {
        let body = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
        format!("event: {}\ndata: {}\n\n", name, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_text_stream() {
        let mut c = ResponsesStreamConverter::new("gpt-5");
        let mut all = Vec::new();

        all.extend(
            c.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n"),
        );
        all.extend(c.push(
            b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
        ));
        all.extend(c.finish());

        let joined = all.join("");
        assert!(joined.contains("event: response.created"));
        assert!(joined.contains("event: response.output_item.added"));
        assert!(joined.contains("event: response.content_part.added"));
        assert!(joined.contains("event: response.output_text.delta"));
        assert!(joined.contains("event: response.output_text.done"));
        assert!(joined.contains("event: response.content_part.done"));
        assert!(joined.contains("event: response.output_item.done"));
        assert!(joined.contains("\"text\":\"Hi\""));
        assert!(joined.contains("event: response.completed"));
        assert!(!joined.contains("event: response.function_call\n"));
    }

    #[test]
    fn test_tool_call_stream() {
        let mut c = ResponsesStreamConverter::new("gpt-5");
        let mut all = Vec::new();

        all.extend(c.push(
            b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"search\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n",
        ));
        all.extend(c.push(
            b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"q\\\":\\\"x\\\"}\"}}]},\"finish_reason\":null}]}\n",
        ));
        all.extend(c.push(
            b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n",
        ));
        all.extend(c.finish());

        let joined = all.join("");
        assert!(joined.contains("event: response.output_item.added"));
        assert!(joined.contains("event: response.function_call_arguments.delta"));
        assert!(joined.contains("event: response.function_call_arguments.done"));
        assert!(joined.contains("event: response.output_item.done"));
        assert!(joined.contains("\"type\":\"function_call\""));
        assert!(joined.contains("\"name\":\"search\""));
        assert!(joined.contains("\"arguments\":\"{\\\"q\\\":\\\"x\\\"}\""));
        assert!(joined.contains("event: response.completed"));
    }

    #[test]
    fn test_finish_error_emits_response_failed() {
        let mut c = ResponsesStreamConverter::new("gpt-5");
        let mut all = Vec::new();

        all.extend(
            c.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n"),
        );
        all.extend(c.finish_error("Upstream stream interrupted: reset"));

        let joined = all.join("");
        assert!(joined.contains("event: response.failed"));
        assert!(joined.contains("\"status\":\"failed\""));
        assert!(joined.contains("\"code\":\"server_error\""));
        assert!(joined.contains("Upstream stream interrupted: reset"));
        assert!(!joined.contains("event: response.completed"));
        assert!(!joined.contains("response.output_text.done"));
        assert!(!joined.contains("response.output_item.done"));

        // 幂等：错误收尾之后不得再补发成功事件。
        assert!(c.finish().is_empty());
        assert!(c.finish_error("again").is_empty());
    }

    #[test]
    fn test_finish_error_before_first_chunk() {
        let mut c = ResponsesStreamConverter::new("gpt-5");
        let joined = c.finish_error("connect failed").join("");
        assert!(joined.contains("event: response.failed"));
        assert!(!joined.contains("event: response.created"));
        assert!(!joined.contains("event: response.completed"));
    }
}
