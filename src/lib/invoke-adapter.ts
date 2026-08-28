// ===== Invoke 适配器 =====
// 在 Tauri invoke 与空数据回退之间分发命令调用。
//
// 架构：
//   - REAL_COMMANDS 集合中的命令 → 真实 Tauri 后端
//   - 其余命令 → 空数据回退（无假数据）
//   - 浏览器开发模式（无 window.__TAURI_INTERNALS__）→ 空数据回退
//
// 当后端某个命令上线后，将其名称添加到 REAL_COMMANDS，即可从空数据回退切换到真实后端。

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { DEFAULT_SETTINGS } from "./constants";

// 当前已注册到 Rust 后端的真实命令
export const REAL_COMMANDS = new Set<string>([
  "greet",
  // 渠道
  "get_channels",
  "create_channel",
  "test_channel",
  "get_channel",
  "update_channel",
  "toggle_channel",
  "delete_channel",
  "reorder_channels",
  // 密钥
  "get_api_keys",
  "create_api_key",
  "update_api_key",
  "delete_api_key",
  // 日志
  "get_logs",
  "get_log",
  "delete_log",
  "get_log_stats",
  "get_mode_stats",
  // 仪表盘
  "get_dashboard_stats",
  // 设置
  "get_settings",
  "save_settings",
  // 服务器
  "restart_server",
  // 安全规则
  "get_builtin_security_rules",
  "update_builtin_security_rule",
  "reset_builtin_security_rules",
  "get_custom_security_rules",
  "create_custom_security_rule",
  "toggle_custom_security_rule",
  "delete_custom_security_rule",
  // 安全发现
  "get_log_findings",
  // 测试台
  "send_test_request",
  // MCP 测试台
  "send_mcp_request",
  // 服务状态
  "get_service_statuses",
  // 导出
  "write_text_file",
  // 知识库
  "get_knowledge_bases",
  "get_knowledge_base",
  "create_knowledge_base",
  "update_knowledge_base",
  "delete_knowledge_base",
  "ask_knowledge_base",
  "get_kb_conversations",
  "delete_kb_conversations",
  "list_kb_documents",
  "get_kb_document",
  "get_kb_document_content",
  "list_kb_document_chunks",
  "upload_kb_document",
  "ingest_kb_document",
  "delete_kb_document",
  "get_kb_stats",
  "build_kb_index",
  "get_kb_index",
  "search_kb",
  "import_kb_source",
  "list_kb_sources",
  "delete_kb_source",
]);

// 环境检测：是否运行在 Tauri WebView 中
const isTauri = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

// ---------- 转换层 ----------
// Rust 后端的 Channel / ApiKey 将 JSON 数组字段以字符串返回，
// 这里将其转换为前端期望的类型化结构。反向（前端 → 后端）在 create 命令时处理。

// 将后端返回的 Channel（JSON 字符串字段）转换为前端类型
const transformChannelFromBackend = (raw: any): any => {
  const parseJson = (s: unknown, fallback: unknown) => {
    if (s == null) return fallback;
    try {
      return typeof s === "string" ? JSON.parse(s) : s;
    } catch {
      return fallback;
    }
  };
  return {
    ...raw,
    models: parseJson(raw.models, []) as string[],
    config: parseJson(raw.config, {}) as Record<string, unknown>,
    model_mapping: parseJson(raw.model_mapping, {}) as Record<string, string>,
  };
};

// ApiKey 的 allowed_models / allowed_channels 为 JSON 字符串 → 解析为数组
const transformApiKeyFromBackend = (raw: any): any => {
  const parseJson = (s: unknown, fallback: unknown) => {
    if (s == null) return fallback;
    try {
      return typeof s === "string" ? JSON.parse(s) : s;
    } catch {
      return fallback;
    }
  };
  return {
    ...raw,
    allowed_models: parseJson(raw.allowed_models, []) as string[],
    allowed_channels: parseJson(raw.allowed_channels, []) as string[],
  };
};

const transformResponse = <T>(cmd: string, data: T): T => {
  if (cmd === "get_channels") {
    return (Array.isArray(data) ? data.map(transformChannelFromBackend) : data) as T;
  }
  if (cmd === "create_channel") {
    return transformChannelFromBackend(data) as T;
  }
  if (cmd === "get_api_keys") {
    return (Array.isArray(data) ? data.map(transformApiKeyFromBackend) : data) as T;
  }
  if (cmd === "create_api_key") {
    return transformApiKeyFromBackend(data) as T;
  }
  // is_stream / is_retry 为 i64 → boolean
  if (cmd === "get_logs" && Array.isArray(data)) {
    return data.map(transformLogFromBackend) as T;
  }
  if (cmd === "get_log" && data && typeof data === "object") {
    return transformLogFromBackend(data) as T;
  }
  // Rust 后端 TestResult 使用 message 字段，前端类型为 error_message，此处做字段映射
  if (cmd === "test_channel" && data && typeof data === "object") {
    const d = data as Record<string, unknown>;
    if ("message" in d && !("error_message" in d)) {
      const { message, ...rest } = d;
      return { ...rest, error_message: message } as T;
    }
  }
  return data;
};

// RequestLog: is_stream / is_retry 转换为布尔值
const transformLogFromBackend = (raw: any): any => {
  return {
    ...raw,
    is_stream: raw.is_stream === 1 || raw.is_stream === true,
    is_retry: raw.is_retry === 1 || raw.is_retry === true,
  };
};

// ---------- 序列化层 ----------
// 前端类型与后端 CreateChannelInput 已对齐（models 为数组、config/model_mapping 为对象），
// 保留此钩子以便未来字段结构变化时在此处转换。
const serializeRequest = (_cmd: string, args: any): any => args;

// ---------- 空数据回退 ----------
// 当后端不可用（浏览器模式或命令未注册到后端）时，
// 返回空数据而非 Mock 假数据，让 UI 展示"暂无数据"等空状态。

const emptyFallback = (cmd: string): Promise<any> => {
  // 列表类 → 空数组
  if (
    cmd === "get_channels" ||
    cmd === "get_api_keys" ||
    cmd === "get_logs" ||
    cmd === "get_log_stats" ||
    cmd === "get_mode_stats" ||
    cmd === "get_builtin_security_rules" ||
    cmd === "get_custom_security_rules" ||
    cmd === "get_log_findings" ||
    cmd === "get_knowledge_bases" ||
    cmd === "get_kb_conversations" ||
    cmd === "list_kb_documents" ||
    cmd === "list_kb_document_chunks" ||
    cmd === "search_kb" ||
    cmd === "list_kb_sources"
  ) {
    return Promise.resolve([]);
  }
  // 单条类 → null
  if (
    cmd === "get_channel" ||
    cmd === "get_log" ||
    cmd === "get_knowledge_base" ||
    cmd === "get_kb_document" ||
    cmd === "get_kb_document_content" ||
    cmd === "get_kb_index"
  ) {
    return Promise.resolve(null);
  }
  // 知识库统计 → 零值
  if (cmd === "get_kb_stats") {
    return Promise.resolve({ doc_count: 0, chunk_count: 0, total_tokens: 0 });
  }
  // RAG 问答 → 空结果
  if (cmd === "ask_knowledge_base") {
    return Promise.resolve({ answer: "", sources: [], usage: null, retrieval_details: null });
  }
  // 仪表盘统计 → 零值
  if (cmd === "get_dashboard_stats") {
    return Promise.resolve({
      today_requests: 0,
      today_total_tokens: 0,
      active_channels: 0,
      avg_latency_ms: 0,
      total_channels: 0,
      total_api_keys: 0,
      total_requests: 0,
      total_tokens: 0,
    });
  }
  // 设置 → 默认值
  if (cmd === "get_settings") {
    return Promise.resolve({ ...DEFAULT_SETTINGS });
  }
  // 测试 → 不可用
  if (cmd === "test_channel") {
    return Promise.resolve({
      success: false,
      latency_ms: 0,
      error_message: "Backend not available",
    });
  }
  if (cmd === "send_test_request") {
    return Promise.resolve({ status: 0, body: null });
  }
  if (cmd === "send_mcp_request") {
    return Promise.resolve({ status: 0, body: null });
  }
  // 服务状态 → 空数组
  if (cmd === "get_service_statuses") {
    return Promise.resolve([]);
  }
  // 写入/删除等 → void
  return Promise.resolve(undefined);
};

export const addRealCommand = (cmd: string) => {
  REAL_COMMANDS.add(cmd);
};

export const removeRealCommand = (cmd: string) => {
  REAL_COMMANDS.delete(cmd);
};

// Tauri 命令 `Result<T, String>` 失败时，invoke 以「字符串」reject（而非 Error 对象），
// 统一在此归一化为 Error，使调用方 `(err as Error).message` 能读到后端真实错误信息。
const normalizeError = (e: unknown): Error => {
  if (e instanceof Error) return e;
  if (typeof e === "string") return new Error(e);
  if (e && typeof e === "object") {
    const m = (e as { message?: unknown }).message;
    if (typeof m === "string" && m) return new Error(m);
  }
  try {
    return new Error(JSON.stringify(e));
  } catch {
    return new Error(String(e));
  }
};

export const invoke = <T>(cmd: string, args?: unknown): Promise<T> => {
  // 浏览器模式：回退到空数据
  if (!isTauri()) {
    return emptyFallback(cmd);
  }

  // Tauri 模式：真实命令走后端，其余回退到空数据
  if (REAL_COMMANDS.has(cmd)) {
    return tauriInvoke<T>(cmd, serializeRequest(cmd, args))
      .then((data) => transformResponse<T>(cmd, data))
      .catch((e) => {
        throw normalizeError(e);
      });
  }

  return emptyFallback(cmd);
};

export { tauriInvoke };
