import { describe, expect, it } from "vitest";
import { normalizeLog, normalizeSession } from "./logNormalize";
import type { LogSessionDetail, RequestLog } from "../types";

const log = (overrides: Partial<RequestLog>): RequestLog => ({
  id: "log-1",
  seq: 1,
  api_key_name: "key",
  channel_name: "deepseek",
  model: "deepseek-flash",
  upstream_model: null,
  mode: "chat",
  status_code: 200,
  prompt_tokens: 10,
  completion_tokens: 5,
  total_tokens: 15,
  duration_ms: 100,
  error_message: null,
  is_stream: false,
  is_retry: false,
  created_at: "2026-11-20T05:00:00.000Z",
  request_body: null,
  forward_body: null,
  response_choices: null,
  response_body: null,
  trace_id: null,
  session_key: "s:test",
  client_session_id: "client-session",
  session_source: "prompt_cache_key",
  session_client: "codex",
  session_title: "测试会话",
  request_kind: "turn",
  history_len: 1,
  risk_level: "none",
  risk_score: 0,
  risk_summary: null,
  security_action: "none",
  sanitized: false,
  blocked_reason: null,
  ...overrides,
});

describe("normalizeLog", () => {
  it("parses chat request and tool calls", () => {
    const result = normalizeLog(
      log({
        request_body: JSON.stringify({
          model: "m",
          messages: [
            { role: "system", content: "system" },
            {
              role: "assistant",
              content: "",
              tool_calls: [
                {
                  id: "call_1",
                  type: "function",
                  function: { name: "search", arguments: '{"q":"x"}' },
                },
              ],
            },
            { role: "tool", tool_call_id: "call_1", content: "result" },
            { role: "user", content: "你好" },
          ],
        }),
        response_body: JSON.stringify({
          choices: [
            { index: 0, message: { role: "assistant", content: "回答" } },
          ],
        }),
      }),
    );
    expect(result.protocol).toBe("chat");
    expect(result.messages.some((message) => message.role === "user")).toBe(true);
    expect(result.messages.some((message) => message.role === "assistant" && message.text === "回答")).toBe(true);
    expect(result.toolCalls[0].name).toBe("search");
    expect(result.toolCalls[0].status).toBe("success");
    expect(result.responseSource).toBe("response_body");
    expect(result.degraded).toBe(false);
  });

  it("parses Anthropic tool_use and tool_result", () => {
    const result = normalizeLog(
      log({
        mode: "messages",
        request_body: JSON.stringify({
          system: [{ type: "text", text: "system" }],
          messages: [
            {
              role: "assistant",
              content: [
                { type: "text", text: "调用工具" },
                { type: "tool_use", id: "call_1", name: "WebFetch", input: { url: "x" } },
              ],
            },
            {
              role: "user",
              content: [
                { type: "tool_result", tool_use_id: "call_1", content: "结果" },
              ],
            },
          ],
        }),
        response_body: JSON.stringify({
          type: "message",
          role: "assistant",
          content: [{ type: "text", text: "最终回答" }],
        }),
      }),
    );
    expect(result.protocol).toBe("messages");
    expect(result.messages.some((message) => message.text === "最终回答")).toBe(true);
    expect(result.toolCalls[0].name).toBe("WebFetch");
    expect(result.toolCalls[0].result?.content).toBe("结果");
    expect(result.toolCalls[0].status).toBe("success");
  });

  it("filters Codex environment/developer messages", () => {
    const result = normalizeLog(
      log({
        mode: "responses",
        request_body: JSON.stringify({
          input: [
            {
              type: "message",
              role: "developer",
              content: [{ type: "input_text", text: "developer" }],
            },
            {
              type: "message",
              role: "user",
              content: [
                {
                  type: "input_text",
                  text: "<environment_context>cwd</environment_context>真实问题",
                },
              ],
            },
            {
              type: "function_call",
              name: "exec_command",
              call_id: "call_1",
              arguments: '{"cmd":"ls"}',
            },
            {
              type: "function_call_output",
              call_id: "call_1",
              output: "file",
            },
          ],
        }),
        response_body: JSON.stringify({
          status: "completed",
          output: [
            {
              type: "message",
              role: "assistant",
              content: [{ type: "output_text", text: "完成" }],
            },
          ],
        }),
      }),
    );
    expect(result.protocol).toBe("responses");
    const visible = result.messages.filter((message) => !message.hidden);
    expect(visible.some((message) => message.text.includes("真实问题"))).toBe(true);
    expect(visible.some((message) => message.role === "developer")).toBe(false);
    expect(result.toolCalls[0].name).toBe("exec_command");
    expect(result.toolCalls[0].status).toBe("success");
  });

  it("degrades old streaming logs without response body", () => {
    const result = normalizeLog(
      log({
        mode: "responses",
        is_stream: true,
        request_body: JSON.stringify({ input: "hi" }),
        response_choices: null,
        response_body: null,
      }),
    );
    expect(result.degraded).toBe(true);
    expect(result.responseSource).toBe("none");
  });

  it("falls back to response_choices for old logs", () => {
    const result = normalizeLog(
      log({
        mode: "messages",
        request_body: JSON.stringify({ messages: [{ role: "user", content: "hi" }] }),
        response_choices: JSON.stringify([
          { index: 0, message: { role: "assistant", content: "旧响应" } },
        ]),
      }),
    );
    expect(result.responseSource).toBe("response_choices");
    expect(result.messages.some((message) => message.text === "旧响应")).toBe(true);
  });
});

describe("normalizeSession", () => {
  const entry = (overrides: Partial<LogSessionDetail["logs"][number]>) => ({
    id: "log-1",
    seq: 1,
    created_at: "2026-11-20T05:00:00.000Z",
    model: "deepseek-flash",
    mode: "responses",
    status_code: 200,
    prompt_tokens: 10,
    completion_tokens: 5,
    total_tokens: 15,
    duration_ms: 100,
    is_retry: 0,
    request_kind: "turn",
    history_len: 1,
    session_title: "测试",
    request_body: null,
    response_body: null,
    response_choices: null,
    ...overrides,
  });

  it("merges snapshot history and per-request responses without duplicates", () => {
    const detail: LogSessionDetail = {
      summary: {
        session_key: "s:test",
        session_title: "测试",
        session_client: "codex",
        models: ["deepseek-flash"],
        first_at: "2026-11-20T05:00:00.000Z",
        last_at: "2026-11-20T05:01:00.000Z",
        request_count: 2,
        turn_count: 2,
        total_tokens: 30,
        last_status_code: 200,
      },
      snapshot_id: "log-2",
      auxiliary_count: 1,
      logs: [
        entry({
          id: "log-1",
          seq: 1,
          response_body: JSON.stringify({
            output: [
              { type: "message", role: "assistant", content: [{ type: "output_text", text: "第一轮回答" }] },
            ],
          }),
        }),
        entry({
          id: "log-2",
          seq: 2,
          history_len: 3,
          request_body: JSON.stringify({
            input: [
              { type: "message", role: "user", content: [{ type: "input_text", text: "第一问" }] },
              { type: "message", role: "assistant", content: [{ type: "output_text", text: "第一轮回答" }] },
              { type: "message", role: "user", content: [{ type: "input_text", text: "第二问" }] },
            ],
          }),
          response_body: JSON.stringify({
            output: [
              { type: "message", role: "assistant", content: [{ type: "output_text", text: "第二轮回答" }] },
            ],
          }),
        }),
        entry({
          id: "aux-1",
          seq: 3,
          request_kind: "auxiliary",
          response_body: JSON.stringify({ output: [] }),
        }),
      ],
    };
    const result = normalizeSession(detail);
    expect(result.turns).toHaveLength(2);
    expect(result.turns[0].messages.some((message) => message.text === "第一问")).toBe(true);
    expect(result.turns[0].messages.some((message) => message.text === "第一轮回答")).toBe(true);
    expect(result.turns[1].messages.some((message) => message.text === "第二问")).toBe(true);
    expect(result.turns[1].messages.some((message) => message.text === "第二轮回答")).toBe(true);
    expect(result.auxiliary).toHaveLength(1);
    // 第一轮回答在快照和第一请求响应里各出现一次，只展示一次。
    expect(result.turns[0].messages.filter((message) => message.text === "第一轮回答")).toHaveLength(1);
  });

  it("marks sessions without response bodies as degraded", () => {
    const detail: LogSessionDetail = {
      summary: {
        session_key: "s:test",
        session_title: "测试",
        session_client: "codex",
        models: ["m"],
        first_at: "2026-11-20T05:00:00.000Z",
        last_at: "2026-11-20T05:00:00.000Z",
        request_count: 1,
        turn_count: 1,
        total_tokens: 0,
        last_status_code: 200,
      },
      snapshot_id: "log-1",
      auxiliary_count: 0,
      logs: [
        entry({
          id: "log-1",
          request_body: JSON.stringify({
            input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "hi" }] }],
          }),
        }),
      ],
    };
    const result = normalizeSession(detail);
    expect(result.degraded).toBe(true);
    expect(result.turns[0].messages[0].text).toBe("hi");
  });
});
