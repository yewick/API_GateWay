//! Anthropic SSE 流式转换：把 OpenAI Chat Completions 的 SSE 增量转换为 Anthropic
//! Messages 的事件序列（message_start / content_block_* / message_delta / message_stop）。
//!
//! 只负责「流式 SSE 互转」与 usage 解析；非流式 JSON 双向转换见 [`super`]。

use serde_json::{json, Value};
use std::collections::HashMap;

/// 单个 tool_call 的流式累积状态。
pub struct ToolCallState {
    pub id: String,
    pub name: String,
    /// 已累积的 arguments 片段。
    pub arguments: String,
    /// 该 tool_use 块在 Anthropic 序列里的 content block 编号。
    pub block_index: usize,
}

/// Anthropic 流式转换的跨 chunk 状态（照文档字段，补全 text_block_index）。
pub struct AnthropicStreamState {
    pub started: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub text_block_open: bool,
    pub text_block_stopped: bool,
    pub text_block_index: usize,
    pub tool_calls: HashMap<usize, ToolCallState>,
    pub next_block_index: usize,
    pub thinking_block_open: bool,
    pub thinking_block_index: usize,
}

impl Default for AnthropicStreamState {
    fn default() -> Self {
        Self {
            started: false,
            input_tokens: 0,
            output_tokens: 0,
            text_block_open: false,
            text_block_stopped: false,
            text_block_index: 0,
            tool_calls: HashMap::new(),
            next_block_index: 0,
            thinking_block_open: false,
            thinking_block_index: 0,
        }
    }
}

/// 有状态转换器：逐块消费 OpenAI SSE 字节，输出 Anthropic SSE 事件字符串。
pub struct AnthropicStreamConverter {
    model: String,
    message_id: String,
    state: AnthropicStreamState,
    /// 未完成的行缓冲（跨 TCP 分片）。
    buffer: Vec<u8>,
    finished: bool,
}

impl AnthropicStreamConverter {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            state: AnthropicStreamState::default(),
            buffer: Vec::new(),
            finished: false,
        }
    }

    /// 喂入一段上游字节，返回本次产生的 Anthropic SSE 事件字符串。
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

    /// 流结束后调用：flush 残余缓冲 + 关闭未收尾块 + message_stop。
    pub fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        self.finished = true;

        // flush 残余行
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

        self.finalize("stop", &mut out);
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
        // 旁路捕获 usage（OpenAI 只在末尾 chunk 带 usage）
        if let Some(usage) = chunk.get("usage") {
            if let Some(p) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                self.state.input_tokens = p;
            }
            if let Some(c) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                self.state.output_tokens = c;
            }
        }

        let choice = chunk.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
        let delta = choice.and_then(|c| c.get("delta"));
        let finish_reason = choice.and_then(|c| c.get("finish_reason")).and_then(|f| f.as_str());

        // 整体消息开始
        if !self.state.started {
            self.state.started = true;
            out.push(self.event("message_start", &json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": Value::Null,
                    "stop_sequence": Value::Null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 }
                }
            })));
        }

        if let Some(delta) = delta {
            // 文本增量
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    self.ensure_text_block_open(out);
                    out.push(self.event("content_block_delta", &json!({
                        "type": "content_block_delta",
                        "index": self.state.text_block_index,
                        "delta": { "type": "text_delta", "text": content }
                    })));
                }
            }

            // 工具调用
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tool_calls {
                    let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                    if !self.state.tool_calls.contains_key(&index) {
                        // 开启新 tool_use 块前，先关闭文本块
                        self.close_text_block(out);
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                        let block_index = self.state.next_block_index;
                        self.state.next_block_index += 1;
                        self.state.tool_calls.insert(index, ToolCallState { id, name, arguments: String::new(), block_index });

                        let st = self.state.tool_calls.get(&index).unwrap();
                        out.push(self.event("content_block_start", &json!({
                            "type": "content_block_start",
                            "index": st.block_index,
                            "content_block": { "type": "tool_use", "id": st.id, "name": st.name, "input": {} }
                        })));
                    }

                    if let Some(arg) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()) {
                        if !arg.is_empty() {
                            if let Some(st) = self.state.tool_calls.get_mut(&index) {
                                st.arguments.push_str(arg);
                                let block_index = st.block_index;
                                out.push(self.event("content_block_delta", &json!({
                                    "type": "content_block_delta",
                                    "index": block_index,
                                    "delta": { "type": "input_json_delta", "partial_json": arg }
                                })));
                            }
                        }
                    }
                }
            }
        }

        if let Some(fr) = finish_reason {
            if !fr.is_empty() && fr != "null" {
                self.finalize(fr, out);
            }
        }
    }

    fn ensure_text_block_open(&mut self, out: &mut Vec<String>) {
        if self.state.text_block_open || self.state.text_block_stopped {
            return;
        }
        self.state.text_block_open = true;
        self.state.text_block_index = self.state.next_block_index;
        self.state.next_block_index += 1;
        out.push(self.event("content_block_start", &json!({
            "type": "content_block_start",
            "index": self.state.text_block_index,
            "content_block": { "type": "text", "text": "" }
        })));
    }

    fn close_text_block(&mut self, out: &mut Vec<String>) {
        if !self.state.text_block_open {
            return;
        }
        self.state.text_block_open = false;
        self.state.text_block_stopped = true;
        out.push(self.event("content_block_stop", &json!({
            "type": "content_block_stop",
            "index": self.state.text_block_index
        })));
    }

    fn finalize(&mut self, finish_reason: &str, out: &mut Vec<String>) {
        if self.finished {
            return;
        }
        self.finished = true;

        self.close_text_block(out);

        // 关闭所有 tool_use 块
        let indices: Vec<usize> = self.state.tool_calls.keys().cloned().collect();
        for idx in indices {
            if let Some(st) = self.state.tool_calls.get(&idx) {
                out.push(self.event("content_block_stop", &json!({
                    "type": "content_block_stop",
                    "index": st.block_index
                })));
            }
        }

        let stop_reason = super::finish_reason_to_stop_reason(finish_reason);
        out.push(self.event("message_delta", &json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": stop_reason,
                "stop_sequence": Value::Null
            },
            "usage": { "output_tokens": self.state.output_tokens }
        })));
        out.push(self.event("message_stop", &json!({
            "type": "message_stop"
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
        let mut c = AnthropicStreamConverter::new("gpt-5");
        let mut all = Vec::new();

        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"finish_reason\":null}]}\n"));
        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n"));
        all.extend(c.push(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n"));
        all.extend(c.finish());

        let joined = all.join("");
        assert!(joined.contains("event: message_start"));
        assert!(joined.contains("event: content_block_start"));
        assert!(joined.contains("text_delta"));
        assert!(joined.contains("Hello"));
        assert!(joined.contains("event: message_delta"));
        assert!(joined.contains("event: message_stop"));
    }

    #[test]
    fn test_tool_call_stream() {
        let mut c = AnthropicStreamConverter::new("gpt-5");
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
        assert!(joined.contains("tool_use"));
        assert!(joined.contains("call_1"));
        assert!(joined.contains("input_json_delta"));
        // stop_reason 应映射为 tool_use
        assert!(joined.contains("\"stop_reason\":\"tool_use\""));
    }
}
