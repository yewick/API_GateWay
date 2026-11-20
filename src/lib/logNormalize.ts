import type {
  LogSessionDetail,
  LogSessionEntry,
  RequestLog,
} from "../types";

export type LogProtocol = "chat" | "messages" | "responses";
export type ViewRole = "user" | "assistant" | "tool" | "system" | "developer";

export interface ViewMessage {
  id: string;
  role: ViewRole;
  text: string;
  thinking: string;
  order: number;
  hidden: boolean;
  hiddenReason?: "system" | "environment" | "system-reminder";
  toolCallId?: string;
  requestId?: string;
}

export interface ViewToolCall {
  id: string;
  name: string;
  argumentsRaw: string;
  input?: unknown;
  result?: { content: string; isError: boolean };
  status: "pending" | "success" | "error";
  order: number;
}

export interface ViewEvent {
  kind:
    | "gateway"
    | "user"
    | "assistant_text"
    | "tool_call"
    | "tool_result"
    | "response"
    | "error";
  label: string;
  detail?: string;
  tone?: "normal" | "success" | "danger" | "muted";
}

export interface NormalizedLog {
  protocol: LogProtocol;
  messages: ViewMessage[];
  hiddenMessages: ViewMessage[];
  toolCalls: ViewToolCall[];
  events: ViewEvent[];
  degraded: boolean;
  degradedReason?: string;
  responseSource: "response_body" | "response_choices" | "none";
  warnings: string[];
}

export interface RequestMeta {
  model: string;
  statusCode: number;
  totalTokens: number;
  durationMs: number;
  requestKind: string;
  createdAt: string;
}

export interface SessionTurn {
  index: number;
  title: string;
  messages: ViewMessage[];
  toolCalls: ViewToolCall[];
  requestIds: string[];
}

export interface NormalizedSession {
  turns: SessionTurn[];
  auxiliary: LogSessionEntry[];
  snapshotId: string | null;
  degraded: boolean;
  degradedReason?: string;
  warnings: string[];
  requestMeta: Record<string, RequestMeta>;
}

const asArray = (value: unknown): unknown[] =>
  Array.isArray(value) ? value : [];

const asObject = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;

export function parseJson(raw: string | null | undefined): unknown | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function stripTagBlocks(text: string, tag: string): string {
  const open = `<${tag}`;
  let result = text;
  for (;;) {
    const start = result.indexOf(open);
    if (start < 0) return result;
    const end = result.indexOf(`</${tag}>`, start);
    if (end < 0) return result;
    result = `${result.slice(0, start)} ${result.slice(end + tag.length + 3)}`;
  }
}

export function stripInjected(text: string): string {
  let result = text;
  for (const tag of [
    "environment_context",
    "user_instructions",
    "system-reminder",
    "total_tokens",
  ]) {
    result = stripTagBlocks(result, tag);
  }
  result = result.replace(/\[Request interrupted by user\]/g, " ");
  return result.split(/\s+/).filter(Boolean).join(" ").trim();
}

function textFromContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => {
        if (typeof part === "string") return part;
        const object = asObject(part);
        if (!object) return "";
        if (typeof object.text === "string") return object.text;
        if (object.type === "tool_result") return textFromContent(object.content);
        return "";
      })
      .filter(Boolean)
      .join("\n");
  }
  const object = asObject(content);
  return object && typeof object.text === "string" ? object.text : "";
}

function hashString(input: string): string {
  let hash = 5381;
  for (let index = 0; index < input.length; index += 1) {
    hash = (hash * 33) ^ input.charCodeAt(index);
  }
  return (hash >>> 0).toString(16);
}

function messageKey(role: string, text: string): string {
  return `${role}|${hashString(stripInjected(text).trim())}`;
}

function toolKey(tool: { id?: string; name: string; argumentsRaw: string }): string {
  return tool.id || `${tool.name}|${tool.argumentsRaw}`;
}

export function detectProtocol(
  mode: string | null | undefined,
  body: unknown,
): LogProtocol {
  const object = asObject(body);
  if (object) {
    if (object.input !== undefined && object.messages === undefined) {
      return "responses";
    }
    if (object.messages !== undefined) {
      const tools = asArray(object.tools).map(asObject);
      const anthropicTools = tools.some((tool) => tool?.input_schema !== undefined);
      if (
        object.system !== undefined ||
        object.metadata !== undefined ||
        anthropicTools
      ) {
        return "messages";
      }
      return "chat";
    }
    if (object.output !== undefined) return "responses";
    if (object.content !== undefined && object.type === "message") {
      return "messages";
    }
    if (object.choices !== undefined) return "chat";
  }
  if (mode === "responses") return "responses";
  if (mode === "messages") return "messages";
  return "chat";
}

interface ParseBucket {
  messages: ViewMessage[];
  toolCalls: ViewToolCall[];
  results: Map<string, { content: string; isError: boolean }>;
  warnings: string[];
}

function pushMessage(
  bucket: ParseBucket,
  message: Omit<ViewMessage, "order" | "hidden" | "thinking"> & {
    thinking?: string;
    hidden?: boolean;
  },
): ViewMessage {
  const added: ViewMessage = {
    id: message.id,
    role: message.role,
    text: message.text,
    thinking: message.thinking ?? "",
    order: bucket.messages.length,
    hidden: message.hidden ?? false,
    hiddenReason: message.hiddenReason,
    toolCallId: message.toolCallId,
    requestId: message.requestId,
  };
  bucket.messages.push(added);
  return added;
}

function addToolCall(
  bucket: ParseBucket,
  call: { id?: string; name: string; argumentsRaw: string },
) {
  const key = toolKey(call);
  if (bucket.toolCalls.some((existing) => toolKey(existing) === key)) return;
  let parsed: unknown;
  try {
    parsed = call.argumentsRaw ? JSON.parse(call.argumentsRaw) : undefined;
  } catch {
    parsed = undefined;
  }
  bucket.toolCalls.push({
    id: call.id || key,
    name: call.name || "unknown",
    argumentsRaw: call.argumentsRaw,
    input: parsed,
    status: "pending",
    order: bucket.messages.length,
  });
}

function addToolResult(
  bucket: ParseBucket,
  callId: string,
  content: string,
  isError: boolean,
) {
  if (!callId) return;
  bucket.results.set(callId, { content, isError });
  if (!bucket.toolCalls.some((tool) => tool.id === callId)) {
    bucket.toolCalls.push({
      id: callId,
      name: "(未匹配结果的调用)",
      argumentsRaw: "",
      status: "pending",
      order: bucket.toolCalls.length,
    });
  }
}

function hiddenReasonFor(role: string, text: string): ViewMessage["hiddenReason"] {
  if (role === "system") return "system";
  if (role === "developer") return "system";
  if (!stripInjected(text)) {
    if (/<environment_context/.test(text)) return "environment";
    if (/<system-reminder/.test(text)) return "system-reminder";
    return "environment";
  }
  return undefined;
}

function parseChatRequest(body: Record<string, unknown>, bucket: ParseBucket) {
  for (const raw of asArray(body.messages)) {
    const message = asObject(raw);
    if (!message) continue;
    const role = String(message.role ?? "user");
    const text = textFromContent(message.content);
    if (role === "tool") {
      pushMessage(bucket, {
        id: `tool-${bucket.messages.length}`,
        role: "tool",
        text,
        hidden: false,
        toolCallId:
          typeof message.tool_call_id === "string"
            ? message.tool_call_id
            : undefined,
      });
      addToolResult(
        bucket,
        String(message.tool_call_id ?? ""),
        text,
        false,
      );
      continue;
    }
    const hidden = role === "system" || role === "developer";
    pushMessage(bucket, {
      id: `${role}-${bucket.messages.length}`,
      role: role as ViewRole,
      text,
      hidden,
      hiddenReason: hidden ? hiddenReasonFor(role, text) : undefined,
    });
    for (const rawCall of asArray(message.tool_calls)) {
      const call = asObject(rawCall);
      if (!call) continue;
      const fn = asObject(call.function);
      addToolCall(bucket, {
        id: typeof call.id === "string" ? call.id : undefined,
        name: typeof fn?.name === "string" ? fn.name : "unknown",
        argumentsRaw:
          typeof fn?.arguments === "string" ? fn.arguments : "",
      });
    }
  }
}

function parseMessagesRequest(body: Record<string, unknown>, bucket: ParseBucket) {
  const system = body.system;
  if (system !== undefined) {
    const text = textFromContent(system);
    pushMessage(bucket, {
      id: "system-0",
      role: "system",
      text,
      hidden: true,
      hiddenReason: "system",
    });
  }
  for (const raw of asArray(body.messages)) {
    const message = asObject(raw);
    if (!message) continue;
    const role = String(message.role ?? "user");
    const content = message.content;
    const blocks = Array.isArray(content) ? content : [];
    const text = textFromContent(content);
    const thinking = blocks
      .map((block) => {
        const object = asObject(block);
        return object?.type === "thinking" && typeof object.thinking === "string"
          ? object.thinking
          : object?.type === "thinking" && typeof object.text === "string"
            ? object.text
            : "";
      })
      .filter(Boolean)
      .join("\n");
    for (const rawBlock of blocks) {
      const block = asObject(rawBlock);
      if (!block) continue;
      if (block.type === "tool_use") {
        addToolCall(bucket, {
          id: typeof block.id === "string" ? block.id : undefined,
          name: typeof block.name === "string" ? block.name : "unknown",
          argumentsRaw:
            typeof block.input === "string"
              ? block.input
              : JSON.stringify(block.input ?? {}),
        });
      }
      if (block.type === "tool_result") {
        const resultText = textFromContent(block.content);
        pushMessage(bucket, {
          id: `tool-result-${bucket.messages.length}`,
          role: "tool",
          text: resultText,
          hidden: false,
          toolCallId:
            typeof block.tool_use_id === "string"
              ? block.tool_use_id
              : undefined,
        });
        addToolResult(
          bucket,
          String(block.tool_use_id ?? ""),
          resultText,
          Boolean(block.is_error),
        );
      }
    }
    if (blocks.length > 0 && blocks.every((block) => asObject(block)?.type === "tool_result")) {
      continue;
    }
    if (!text && !thinking) continue;
    const cleaned = stripInjected(text);
    const hidden =
      role === "system" ||
      (role === "user" && !cleaned && blocks.every((b) => asObject(b)?.type === "tool_result"));
    pushMessage(bucket, {
      id: `${role}-${bucket.messages.length}`,
      role: role as ViewRole,
      text: cleaned || text,
      thinking,
      hidden,
      hiddenReason: hidden ? hiddenReasonFor(role, text) : undefined,
    });
  }
}

function parseResponsesRequest(body: Record<string, unknown>, bucket: ParseBucket) {
  const input = body.input;
  if (typeof input === "string") {
    const cleaned = stripInjected(input);
    pushMessage(bucket, {
      id: "user-0",
      role: "user",
      text: cleaned || input,
      hidden: false,
    });
    return;
  }
  for (const raw of asArray(input)) {
    const item = asObject(raw);
    if (!item) continue;
    const type = String(item.type ?? "message");
    if (type === "function_call") {
      addToolCall(bucket, {
        id: typeof item.call_id === "string" ? item.call_id : undefined,
        name: typeof item.name === "string" ? item.name : "unknown",
        argumentsRaw:
          typeof item.arguments === "string"
            ? item.arguments
            : JSON.stringify(item.arguments ?? {}),
      });
      continue;
    }
    if (type === "function_call_output") {
      const resultText =
        typeof item.output === "string"
          ? item.output
          : JSON.stringify(item.output ?? "");
      pushMessage(bucket, {
        id: `tool-result-${bucket.messages.length}`,
        role: "tool",
        text: resultText,
        hidden: false,
        toolCallId:
          typeof item.call_id === "string" ? item.call_id : undefined,
      });
      addToolResult(bucket, String(item.call_id ?? ""), resultText, false);
      continue;
    }
    if (type === "reasoning") continue;
    const role = String(item.role ?? "user");
    const text = textFromContent(item.content);
    const cleaned = stripInjected(text);
    const hidden =
      role === "developer" || role === "system" || (!cleaned && role === "user");
    pushMessage(bucket, {
      id: `${role}-${bucket.messages.length}`,
      role: role as ViewRole,
      text: cleaned || text,
      hidden,
      hiddenReason: hidden ? hiddenReasonFor(role, text) : undefined,
    });
  }
}

function parseRequest(
  protocol: LogProtocol,
  body: Record<string, unknown>,
  bucket: ParseBucket,
) {
  if (protocol === "responses") parseResponsesRequest(body, bucket);
  else if (protocol === "messages") parseMessagesRequest(body, bucket);
  else parseChatRequest(body, bucket);
}

interface ParsedResponse {
  text: string;
  thinking: string;
  toolCalls: { id?: string; name: string; argumentsRaw: string }[];
}

function parseChatMessage(message: Record<string, unknown>): ParsedResponse {
  const text = textFromContent(message.content);
  const thinking =
    typeof message.reasoning_content === "string"
      ? message.reasoning_content
      : "";
  const toolCalls = asArray(message.tool_calls)
    .map(asObject)
    .filter(Boolean)
    .map((call) => {
      const fn = asObject(call!.function);
      return {
        id: typeof call!.id === "string" ? call!.id : undefined,
        name: typeof fn?.name === "string" ? fn.name : "unknown",
        argumentsRaw:
          typeof fn?.arguments === "string" ? fn.arguments : "",
      };
    });
  return { text, thinking, toolCalls };
}

function parseChoices(choices: unknown): ParsedResponse | null {
  const first = asObject(asArray(choices)[0]);
  const message = asObject(first?.message);
  return message ? parseChatMessage(message) : null;
}

function parseResponseBody(
  protocol: LogProtocol,
  body: Record<string, unknown>,
): ParsedResponse | null {
  if (protocol === "chat") {
    return parseChoices(body.choices);
  }
  if (protocol === "messages") {
    const content = body.content;
    const text = textFromContent(content);
    const thinking = asArray(content)
      .map(asObject)
      .filter((block) => block?.type === "thinking")
      .map((block) => String(block?.thinking ?? block?.text ?? ""))
      .filter(Boolean)
      .join("\n");
    const toolCalls = asArray(content)
      .map(asObject)
      .filter((block) => block?.type === "tool_use")
      .map((block) => ({
        id: typeof block!.id === "string" ? block!.id : undefined,
        name: typeof block!.name === "string" ? block!.name : "unknown",
        argumentsRaw:
          typeof block!.input === "string"
            ? block!.input
            : JSON.stringify(block!.input ?? {}),
      }));
    if (!text && !thinking && toolCalls.length === 0) return null;
    return { text, thinking, toolCalls };
  }
  // responses
  const output = asArray(body.output);
  if (Array.isArray(body.choices)) return parseChoices(body.choices);
  const messages = output.map(asObject).filter(Boolean) as Record<string, unknown>[];
  const text = messages
    .filter((item) => item.type === "message")
    .map((item) => textFromContent(item.content))
    .filter(Boolean)
    .join("\n");
  const toolCalls = messages
    .filter((item) => item.type === "function_call")
    .map((item) => ({
      id: typeof item.call_id === "string" ? item.call_id : undefined,
      name: typeof item.name === "string" ? item.name : "unknown",
      argumentsRaw:
        typeof item.arguments === "string"
          ? item.arguments
          : JSON.stringify(item.arguments ?? {}),
    }));
  if (!text && toolCalls.length === 0) return null;
  return { text, thinking: "", toolCalls };
}

function attachToolResults(bucket: ParseBucket) {
  for (const tool of bucket.toolCalls) {
    const result = bucket.results.get(tool.id);
    if (result) {
      tool.result = result;
      tool.status = result.isError ? "error" : "success";
    }
  }
}

function buildEvents(
  log: RequestLog,
  protocol: LogProtocol,
  bucket: ParseBucket,
): ViewEvent[] {
  const events: ViewEvent[] = [
    {
      kind: "gateway",
      label: `${protocol === "chat" ? "Chat" : protocol === "messages" ? "Messages" : "Responses"} 请求`,
      detail: `${log.model} · ${log.is_stream ? "流式" : "非流式"}${log.channel_name ? ` · ${log.channel_name}` : ""}`,
    },
  ];
  for (const message of bucket.messages) {
    if (message.hidden) continue;
    if (message.role === "user") {
      events.push({
        kind: "user",
        label: message.text.slice(0, 120) || "用户消息",
        tone: "normal",
      });
    } else if (message.role === "assistant" && message.text) {
      events.push({
        kind: "assistant_text",
        label: message.text.slice(0, 120),
        tone: "normal",
      });
    }
  }
  for (const tool of bucket.toolCalls) {
    events.push({
      kind: "tool_call",
      label: `调用 ${tool.name}`,
      detail: tool.argumentsRaw.slice(0, 160),
      tone: "normal",
    });
    if (tool.result) {
      events.push({
        kind: "tool_result",
        label: `${tool.name} 结果`,
        detail: tool.result.content.slice(0, 160),
        tone: tool.result.isError ? "danger" : "success",
      });
    }
  }
  events.push({
    kind: log.error_message ? "error" : "response",
    label: log.error_message
      ? "请求失败"
      : `响应 ${log.status_code}`,
    detail: log.error_message
      ? log.error_message
      : `${log.duration_ms}ms · ${log.total_tokens} tokens`,
    tone: log.error_message ? "danger" : log.status_code < 400 ? "success" : "danger",
  });
  return events;
}

export function normalizeLog(log: RequestLog): NormalizedLog {
  const warnings: string[] = [];
  const requestBody = parseJson(log.request_body);
  if (log.request_body && requestBody === null) {
    warnings.push("请求体不是合法 JSON，已按原文保留。");
  }
  const requestObject = asObject(requestBody) ?? {};
  const protocol = detectProtocol(log.mode, requestObject);
  const bucket: ParseBucket = {
    messages: [],
    toolCalls: [],
    results: new Map(),
    warnings,
  };
  parseRequest(protocol, requestObject, bucket);

  const responseBody = parseJson(log.response_body);
  const choicesBody = parseJson(log.response_choices);
  let responseSource: NormalizedLog["responseSource"] = "none";
  let parsedResponse: ParsedResponse | null = null;
  if (responseBody && asObject(responseBody)) {
    responseSource = "response_body";
    parsedResponse = parseResponseBody(protocol, asObject(responseBody)!);
  } else if (choicesBody) {
    responseSource = "response_choices";
    parsedResponse = parseChoices(choicesBody);
  }

  if (parsedResponse) {
    if (parsedResponse.text || parsedResponse.thinking) {
      pushMessage(bucket, {
        id: `assistant-${bucket.messages.length}`,
        role: "assistant",
        text: parsedResponse.text,
        thinking: parsedResponse.thinking,
        hidden: false,
      });
    }
    for (const call of parsedResponse.toolCalls) addToolCall(bucket, call);
  }
  attachToolResults(bucket);

  const degraded = !parsedResponse && log.is_stream && !log.response_body;
  return {
    protocol,
    messages: bucket.messages,
    hiddenMessages: bucket.messages.filter((message) => message.hidden),
    toolCalls: bucket.toolCalls,
    events: buildEvents(log, protocol, bucket),
    degraded,
    degradedReason: degraded
      ? "旧版本流式日志未保存本轮响应，只能还原请求历史与工具上下文。"
      : undefined,
    responseSource,
    warnings,
  };
}

export function normalizeSession(detail: LogSessionDetail): NormalizedSession {
  const warnings: string[] = [];
  const snapshot = detail.logs.find(
    (entry) => entry.id === detail.snapshot_id && entry.request_body,
  ) ?? detail.logs.find((entry) => entry.request_body);
  const auxiliary = detail.logs.filter((entry) => entry.request_kind !== "turn");
  const mainLogs = detail.logs.filter((entry) => entry.request_kind === "turn");

  if (!snapshot) {
    return {
      turns: [],
      auxiliary,
      snapshotId: detail.snapshot_id,
      degraded: true,
      degradedReason: "该会话没有可用的请求快照，无法还原对话历史。",
      warnings,
      requestMeta: {},
    };
  }

  const snapshotBody = parseJson(snapshot.request_body);
  if (snapshotBody === null) {
    warnings.push("主快照请求体不是合法 JSON。");
  }
  const snapshotObject = asObject(snapshotBody) ?? {};
  const protocol = detectProtocol(snapshot.mode, snapshotObject);
  const bucket: ParseBucket = {
    messages: [],
    toolCalls: [],
    results: new Map(),
    warnings,
  };
  parseRequest(protocol, snapshotObject, bucket);

  const existingMessages = new Set(
    bucket.messages.map((message) => messageKey(message.role, message.text)),
  );
  const existingTools = new Set(bucket.toolCalls.map((tool) => toolKey(tool)));

  let sawResponse = false;
  for (const entry of mainLogs) {
    const responseBody = parseJson(entry.response_body);
    const choicesBody = parseJson(entry.response_choices);
    let parsed: ParsedResponse | null = null;
    if (responseBody && asObject(responseBody)) {
      sawResponse = true;
      parsed = parseResponseBody(protocol, asObject(responseBody)!);
    } else if (choicesBody) {
      sawResponse = true;
      parsed = parseChoices(choicesBody);
    }
    if (!parsed) continue;
    const key = messageKey("assistant", parsed.text);
    if (
      (parsed.text || parsed.thinking) &&
      !existingMessages.has(key)
    ) {
      existingMessages.add(key);
      pushMessage(bucket, {
        id: `assistant-${entry.id}`,
        role: "assistant",
        text: parsed.text,
        thinking: parsed.thinking,
        hidden: false,
        requestId: entry.id,
      });
    }
    for (const call of parsed.toolCalls) {
      if (existingTools.has(toolKey(call))) continue;
      existingTools.add(toolKey(call));
      addToolCall(bucket, call);
    }
  }
  attachToolResults(bucket);

  const requestMeta: Record<string, RequestMeta> = {};
  for (const entry of detail.logs) {
    requestMeta[entry.id] = {
      model: entry.model,
      statusCode: entry.status_code,
      totalTokens: entry.total_tokens,
      durationMs: entry.duration_ms,
      requestKind: entry.request_kind,
      createdAt: entry.created_at,
    };
  }

  // 按用户消息切轮；工具调用按 order 归入所在轮。
  const turns: SessionTurn[] = [];
  let current: SessionTurn | null = null;
  const pending: ViewMessage[] = [];
  for (const message of bucket.messages) {
    if (message.role === "user" && !message.hidden) {
      current = {
        index: turns.length + 1,
        title: message.text.slice(0, 80) || `第 ${turns.length + 1} 轮`,
        messages: pending.splice(0),
        toolCalls: [],
        requestIds: [],
      };
      turns.push(current);
    }
    if (!current) {
      pending.push(message);
      continue;
    }
    current.messages.push(message);
    if (message.requestId && !current.requestIds.includes(message.requestId)) {
      current.requestIds.push(message.requestId);
    }
  }
  if (turns.length === 0 && pending.length > 0) {
    turns.push({
      index: 1,
      title: "系统与上下文",
      messages: pending,
      toolCalls: [],
      requestIds: [],
    });
  }
  for (const turn of turns) {
    const orders = turn.messages.map((message) => message.order);
    const minOrder = Math.min(...orders);
    const maxOrder = Math.max(...orders);
    turn.toolCalls = bucket.toolCalls.filter(
      (tool) => tool.order >= minOrder && tool.order <= maxOrder + 1,
    );
  }

  const degraded = !sawResponse;
  return {
    turns,
    auxiliary,
    snapshotId: snapshot.id,
    degraded,
    degradedReason: degraded
      ? "该会话的流式响应还没有保存完整内容，当前只能还原请求历史。"
      : undefined,
    warnings,
    requestMeta,
  };
}
