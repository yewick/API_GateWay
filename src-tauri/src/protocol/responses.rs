//! Responses SSE 流式转换：把 OpenAI Chat Completions 的 SSE 增量转换为 Responses
//! 的事件序列（response.created / response.output_text.delta / response.function_call /
//! response.function_call_arguments.delta / response.completed）。
//!
//! 只负责「流式 SSE 互转」与 usage 解析；非流式 JSON 双向转换见 [`super`]。

use serde_json::{json, Value};

/// 有状态转换器：逐块消费 OpenAI SSE 字节，输出 Responses SSE 事件字符串。
pub struct ResponsesStreamConverter {
    model: String,
    response_id: String,
    /// 未完成的行缓冲（跨 TCP 分片）。
    buffer: Vec<u8>,
    started: bool,
    finished: bool,
    /// 当前文本输出的 item 标识。
    msg_item_id: String,
    msg_output_index: usize,
    /// 工具调用状态：index → (item_id, call_id, output_index)。
    tool_calls: std::collections::HashMap<usize, (String, String, usize)>,
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
            msg_item_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            msg_output_index: 0,
            tool_calls: std::collections::HashMap::new(),
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
        self.finished = true;

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

        let choice = chunk.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
        let delta = choice.and_then(|c| c.get("delta"));
        let finish_reason = choice.and_then(|c| c.get("finish_reason")).and_then(|f| f.as_str());

        if !self.started {
            self.started = true;
            self.msg_output_index = self.next_output_index;
            self.next_output_index += 1;
            out.push(self.event("response.created", &json!({
                "type": "response.created",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "status": "in_progress",
                    "model": self.model,
                    "output": []
                }
            })));
        }

        if let Some(delta) = delta {
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    out.push(self.event("response.output_text.delta", &json!({
                        "type": "response.output_text.delta",
                        "item_id": self.msg_item_id,
                        "output_index": self.msg_output_index,
                        "content_index": 0,
                        "delta": content
                    })));
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tool_calls {
                    let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                    if !self.tool_calls.contains_key(&index) {
                        let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                        let item_id = format!("fc_{}", uuid::Uuid::new_v4().simple());
                        let output_index = self.next_output_index;
                        self.next_output_index += 1;
                        self.tool_calls.insert(index, (item_id.clone(), call_id.clone(), output_index));

                        out.push(self.event("response.function_call", &json!({
                            "type": "response.function_call",
                            "item_id": item_id,
                            "output_index": output_index,
                            "call_id": call_id,
                            "name": name,
                            "arguments": ""
                        })));
                    }

                    if let Some(arg) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()) {
                        if !arg.is_empty() {
                            if let Some((item_id, _, output_index)) = self.tool_calls.get(&index) {
                                out.push(self.event("response.function_call_arguments.delta", &json!({
                                    "type": "response.function_call_arguments.delta",
                                    "item_id": item_id,
                                    "output_index": output_index,
                                    "delta": arg
                                })));
                            }
                        }
                    }
                }
            }
        }

        if let Some(fr) = finish_reason {
            if !fr.is_empty() && fr != "null" {
                self.complete(fr, out);
            }
        }
    }

    fn complete(&mut self, finish_reason: &str, out: &mut Vec<String>) {
        if self.finished {
            return;
        }
        self.finished = true;

        let mut output_items: Vec<Value> = Vec::new();
        output_items.push(json!({
            "id": self.msg_item_id,
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": ""}],
            "status": "completed"
        }));

        let mut tool_items: Vec<Value> = self
            .tool_calls
            .iter()
            .map(|(_, (item_id, call_id, _))| {
                json!({
                    "id": item_id,
                    "type": "function_call",
                    "call_id": call_id,
                    "status": "completed"
                })
            })
            .collect();
        output_items.append(&mut tool_items);

        out.push(self.event("response.completed", &json!({
            "type": "response.completed",
            "response": {
                "id": self.response_id,
                "object": "response",
                "status": "completed",
                "model": self.model,
                "output": output_items,
                "finish_reason": finish_reason,
                "usage": {
                    "input_tokens": self.input_tokens,
                    "output_tokens": self.output_tokens,
                    "total_tokens": self.input_tokens + self.output_tokens
                }
            }
        })));
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

        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n"));
        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n"));
        all.extend(c.finish());

        let joined = all.join("");
        assert!(joined.contains("event: response.created"));
        assert!(joined.contains("event: response.output_text.delta"));
        assert!(joined.contains("\"delta\":\"Hi\""));
        assert!(joined.contains("event: response.completed"));
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
        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n"));
        all.extend(c.finish());

        let joined = all.join("");
        assert!(joined.contains("event: response.function_call"));
        assert!(joined.contains("event: response.function_call_arguments.delta"));
        assert!(joined.contains("event: response.completed"));
    }
}
