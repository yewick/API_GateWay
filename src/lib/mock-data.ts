// ===== Mock 数据层 =====
// 模拟 Tauri 后端的命令返回。当后端命令未注册时，invoke-adapter 会路由到这里。
// 后端命令上线后，只需从 invoke-adapter 的 REAL_COMMANDS 集合移除对应名称即可。

import type {
  Channel,
  ApiKey,
  RequestLog,
  DashboardStats,
  Settings,
  CreateChannelInput,
  UpdateChannelInput,
  CreateApiKeyInput,
  LogStats,
  TestChannelResult,
} from "../types";
import { DEFAULT_SETTINGS } from "./constants";

// ---------- 工具函数 ----------

let seqCounter = 1000;

const genId = (): string => {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `id-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
};

const nowIso = (): string => new Date().toISOString();

const daysAgo = (days: number, hour = 10, minute = 30): string => {
  const d = new Date();
  d.setDate(d.getDate() - days);
  d.setHours(hour, minute, 0, 0);
  return d.toISOString();
};

const delay = (ms = 150): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

// ---------- 种子数据：渠道 ----------

const seedChannels: Channel[] = [
  {
    id: "c-001",
    name: "OpenAI 主渠道",
    type: "openai",
    base_url: "https://api.openai.com/v1",
    api_key: "sk-proj-xxxxxxxxxxxxxxxxxxxxxxxx",
    models: ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo"],
    status: 1,
    priority: 100,
    weight: 1,
    config: {},
    model_mapping: {},
    created_at: daysAgo(20),
    updated_at: daysAgo(1),
    last_test_at: daysAgo(1, 9, 15),
    last_test_ok: 1,
  },
  {
    id: "c-002",
    name: "DeepSeek 官方",
    type: "deepseek",
    base_url: "https://api.deepseek.com",
    api_key: "sk-xxxxxxxxxxxxxxxxxxxxxxxx",
    models: ["deepseek-v4-flash", "deepseek-v4-pro"],
    status: 1,
    priority: 90,
    weight: 1,
    config: {},
    model_mapping: {},
    created_at: daysAgo(15),
    updated_at: daysAgo(2),
    last_test_at: daysAgo(2, 14, 20),
    last_test_ok: 1,
  },
  {
    id: "c-003",
    name: "Anthropic Claude 备用",
    type: "claude",
    base_url: "https://api.anthropic.com/v1",
    api_key: "sk-ant-xxxxxxxxxxxxxxxxxxxxxxxx",
    models: ["claude-sonnet-4-5", "claude-haiku-4-5"],
    status: 0,
    priority: 80,
    weight: 1,
    config: {},
    model_mapping: {},
    created_at: daysAgo(10),
    updated_at: daysAgo(3),
    last_test_at: daysAgo(3, 11, 5),
    last_test_ok: 0,
  },
  {
    id: "c-004",
    name: "Gemini 国际",
    type: "gemini",
    base_url: "https://generativelanguage.googleapis.com/v1",
    api_key: "AIza-xxxxxxxxxxxxxxxxxxxxxxxx",
    models: ["gemini-2.0-flash", "gemini-2.0-pro"],
    status: 1,
    priority: 70,
    weight: 2,
    config: {},
    model_mapping: {},
    created_at: daysAgo(8),
    updated_at: daysAgo(1),
    last_test_at: daysAgo(1, 16, 45),
    last_test_ok: 1,
  },
  {
    id: "c-005",
    name: "本地 Ollama",
    type: "ollama",
    base_url: "http://localhost:11434/v1",
    api_key: "ollama",
    models: ["llama3.1", "qwen2.5"],
    status: 1,
    priority: 50,
    weight: 1,
    config: {},
    model_mapping: {},
    created_at: daysAgo(5),
    updated_at: daysAgo(1),
    last_test_at: null,
    last_test_ok: null,
  },
];

// ---------- 种子数据：密钥 ----------

const seedApiKeys: ApiKey[] = [
  {
    id: "k-001",
    name: "默认密钥",
    key: "sk-yeapi-a1b2c3d4e5f60718293a4b5c6d7e8f90",
    status: 1,
    allowed_models: ["gpt-4o", "deepseek-v4-flash"],
    allowed_channels: [],
    quota_limit: 1000000,
    quota_used: 234500,
    expires_at: null,
    created_at: daysAgo(20),
    updated_at: daysAgo(4),
  },
  {
    id: "k-002",
    name: "测试账号",
    key: "sk-yeapi-11112222333344445555666677778888",
    status: 1,
    allowed_models: [],
    allowed_channels: ["c-001"],
    quota_limit: 10000,
    quota_used: 6780,
    expires_at: daysAgo(-30), // 30 天后过期
    created_at: daysAgo(12),
    updated_at: daysAgo(2),
  },
  {
    id: "k-003",
    name: "已禁用密钥",
    key: "sk-yeapi-99998888777766665555444433332222",
    status: 0,
    allowed_models: [],
    allowed_channels: [],
    quota_limit: -1,
    quota_used: 12000,
    expires_at: null,
    created_at: daysAgo(18),
    updated_at: daysAgo(6),
  },
];

// ---------- 种子数据：请求日志 ----------

const logSeedSpecs: Array<Partial<RequestLog>> = [
  { api_key_name: "默认密钥", channel_name: "OpenAI 主渠道", model: "gpt-4o", status_code: 200, prompt_tokens: 320, completion_tokens: 480, total_tokens: 800, duration_ms: 1250, is_stream: true, risk_level: "low", risk_score: 5 },
  { api_key_name: "测试账号", channel_name: "OpenAI 主渠道", model: "gpt-4o-mini", status_code: 200, prompt_tokens: 150, completion_tokens: 220, total_tokens: 370, duration_ms: 640, is_stream: false, risk_level: "none", risk_score: 0 },
  { api_key_name: "默认密钥", channel_name: "DeepSeek 官方", model: "deepseek-v4-flash", status_code: 200, prompt_tokens: 512, completion_tokens: 1024, total_tokens: 1536, duration_ms: 2100, is_stream: true, risk_level: "medium", risk_score: 25 },
  { api_key_name: "默认密钥", channel_name: "OpenAI 主渠道", model: "gpt-4o", status_code: 429, prompt_tokens: 0, completion_tokens: 0, total_tokens: 0, duration_ms: 88, error_message: "rate limit exceeded", is_stream: false, risk_level: "none", risk_score: 0 },
  { api_key_name: "测试账号", channel_name: "Gemini 国际", model: "gemini-2.0-flash", status_code: 200, prompt_tokens: 210, completion_tokens: 340, total_tokens: 550, duration_ms: 980, is_stream: false, risk_level: "low", risk_score: 8 },
  { api_key_name: "已禁用密钥", channel_name: "DeepSeek 官方", model: "deepseek-v4-pro", status_code: 200, prompt_tokens: 800, completion_tokens: 2000, total_tokens: 2800, duration_ms: 3400, is_stream: true, risk_level: "medium", risk_score: 30 },
  { api_key_name: "默认密钥", channel_name: "OpenAI 主渠道", model: "gpt-4o-mini", status_code: 200, prompt_tokens: 95, completion_tokens: 120, total_tokens: 215, duration_ms: 420, is_stream: false, risk_level: "none", risk_score: 0 },
  { api_key_name: "测试账号", channel_name: "Anthropic Claude 备用", model: "claude-sonnet-4-5", status_code: 401, prompt_tokens: 0, completion_tokens: 0, total_tokens: 0, duration_ms: 250, error_message: "invalid api key", is_stream: false, risk_level: "high", risk_score: 60 },
  { api_key_name: "默认密钥", channel_name: "DeepSeek 官方", model: "deepseek-v4-flash", status_code: 200, prompt_tokens: 400, completion_tokens: 600, total_tokens: 1000, duration_ms: 1500, is_stream: true, risk_level: "low", risk_score: 10 },
  { api_key_name: "默认密钥", channel_name: "Gemini 国际", model: "gemini-2.0-pro", status_code: 200, prompt_tokens: 650, completion_tokens: 900, total_tokens: 1550, duration_ms: 2300, is_stream: true, risk_level: "medium", risk_score: 20 },
  { api_key_name: "测试账号", channel_name: "OpenAI 主渠道", model: "gpt-4o", status_code: 200, prompt_tokens: 300, completion_tokens: 400, total_tokens: 700, duration_ms: 1100, is_stream: false, risk_level: "none", risk_score: 0 },
  { api_key_name: "默认密钥", channel_name: "OpenAI 主渠道", model: "gpt-4o", status_code: 500, prompt_tokens: 120, completion_tokens: 0, total_tokens: 120, duration_ms: 3200, error_message: "upstream internal error", is_stream: false, risk_level: "high", risk_score: 55 },
  { api_key_name: "默认密钥", channel_name: "DeepSeek 官方", model: "deepseek-v4-pro", status_code: 200, prompt_tokens: 1024, completion_tokens: 2048, total_tokens: 3072, duration_ms: 5200, is_stream: true, risk_level: "low", risk_score: 12 },
  { api_key_name: "测试账号", channel_name: "本地 Ollama", model: "llama3.1", status_code: 200, prompt_tokens: 180, completion_tokens: 260, total_tokens: 440, duration_ms: 3300, is_stream: true, risk_level: "none", risk_score: 0 },
  { api_key_name: "默认密钥", channel_name: "OpenAI 主渠道", model: "gpt-4o-mini", status_code: 200, prompt_tokens: 250, completion_tokens: 180, total_tokens: 430, duration_ms: 520, is_stream: false, risk_level: "low", risk_score: 3 },
  { api_key_name: "已禁用密钥", channel_name: "Gemini 国际", model: "gemini-2.0-flash", status_code: 403, prompt_tokens: 0, completion_tokens: 0, total_tokens: 0, duration_ms: 300, error_message: "permission denied", is_stream: false, risk_level: "high", risk_score: 70 },
];

const buildSeedLogs = (): RequestLog[] => {
  return logSeedSpecs.map((spec, i) => {
    const created = daysAgo(i % 7, 9 + (i % 10), 15 + i);
    const risk = spec.risk_level ?? "none";
    return {
      id: `l-${i + 1}`,
      seq: seqCounter++,
      api_key_name: spec.api_key_name ?? "默认密钥",
      channel_name: spec.channel_name ?? "OpenAI 主渠道",
      model: spec.model ?? "gpt-4o",
      upstream_model: spec.model ?? "gpt-4o",
      mode: "chat",
      status_code: spec.status_code ?? 200,
      prompt_tokens: spec.prompt_tokens ?? 0,
      completion_tokens: spec.completion_tokens ?? 0,
      total_tokens: spec.total_tokens ?? 0,
      duration_ms: spec.duration_ms ?? 0,
      error_message: spec.error_message ?? null,
      is_stream: spec.is_stream ?? false,
      is_retry: spec.status_code && spec.status_code >= 500 ? true : false,
      created_at: created,
      request_body: `{\n  "model": "${spec.model ?? "gpt-4o"}",\n  "messages": [\n    { "role": "user", "content": "你好，请介绍一下你自己" }\n  ],\n  "stream": ${spec.is_stream ?? false}\n}`,
      risk_level: risk,
      risk_score: spec.risk_score ?? 0,
      risk_summary: risk === "none" ? null : "检测到潜在风险内容",
      security_action: risk === "high" ? "blocked" : risk === "medium" ? "sanitized" : "none",
      sanitized: risk === "medium",
      blocked_reason: risk === "high" ? "命中安全策略规则" : null,
    };
  });
};

// ---------- 内存数据存储 ----------

let channels: Channel[] = [...seedChannels];
let apiKeys: ApiKey[] = [...seedApiKeys];
let requestLogs: RequestLog[] = buildSeedLogs();
let settings: Settings = { ...DEFAULT_SETTINGS };

// ---------- Mock 处理函数 ----------

// 用于在真实 Tauri 模式下，将后端返回的 Channel（JSON 字符串字段）转换为前端类型
export const transformChannelFromBackend = (raw: any): Channel => {
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

export const mockHandlers: Record<string, (args?: any) => Promise<any>> = {
  // ===== 渠道 =====
  get_channels: async () => {
    await delay();
    return [...channels];
  },
  get_channel: async ({ id }) => {
    await delay(80);
    return channels.find((c) => c.id === id) ?? null;
  },
  create_channel: async ({ input }: { input: CreateChannelInput }) => {
    await delay(200);
    const now = nowIso();
    const channel: Channel = {
      id: genId(),
      name: input.name,
      type: input.type,
      base_url: input.base_url,
      api_key: input.api_key,
      models: input.models ?? [],
      status: 1,
      priority: input.priority ?? 0,
      weight: input.weight ?? 1,
      config: input.config ?? {},
      model_mapping: input.model_mapping ?? {},
      created_at: now,
      updated_at: now,
      last_test_at: null,
      last_test_ok: null,
    };
    channels.unshift(channel);
    return channel;
  },
  update_channel: async ({ input }: { input: UpdateChannelInput }) => {
    await delay(200);
    const idx = channels.findIndex((c) => c.id === input.id);
    if (idx === -1) throw new Error("渠道不存在");
    const updated: Channel = {
      ...channels[idx],
      ...input,
      models: input.models ?? channels[idx].models,
      config: input.config ?? channels[idx].config,
      model_mapping: input.model_mapping ?? channels[idx].model_mapping,
      updated_at: nowIso(),
    };
    channels[idx] = updated;
    return updated;
  },
  toggle_channel: async ({ id, status }: { id: string; status: number }) => {
    await delay(80);
    const c = channels.find((x) => x.id === id);
    if (!c) throw new Error("渠道不存在");
    c.status = status;
    c.updated_at = nowIso();
  },
  delete_channel: async ({ id }: { id: string }) => {
    await delay(150);
    channels = channels.filter((c) => c.id !== id);
  },
  test_channel: async ({ id }: { id: string }): Promise<TestChannelResult> => {
    await delay(600);
    const c = channels.find((x) => x.id === id);
    if (!c) throw new Error("渠道不存在");
    // Mock 模拟测试结果：Ollama 本地渠道成功概率高，Claude 渠道模拟失败
    const success = c.type !== "claude";
    const latency = Math.floor(150 + Math.random() * 400);
    c.last_test_at = nowIso();
    c.last_test_ok = success ? 1 : 0;
    return {
      success,
      latency_ms: latency,
      error_message: success ? undefined : "Connection failed: 401 Unauthorized",
    };
  },

  // ===== 密钥 =====
  get_api_keys: async () => {
    await delay();
    return [...apiKeys];
  },
  create_api_key: async ({ input }: { input: CreateApiKeyInput }) => {
    await delay(250);
    const key = generateApiKey();
    const now = nowIso();
    const apiKey: ApiKey = {
      id: genId(),
      name: input.name,
      key,
      status: 1,
      allowed_models: input.allowed_models ?? [],
      allowed_channels: input.allowed_channels ?? [],
      quota_limit: input.quota_limit ?? -1,
      quota_used: 0,
      expires_at: input.expires_at ?? null,
      created_at: now,
      updated_at: now,
    };
    apiKeys.unshift(apiKey);
    return apiKey;
  },
  update_api_key: async ({ input }: { input: { id: string; status?: number } }) => {
    await delay(120);
    const k = apiKeys.find((x) => x.id === input.id);
    if (!k) throw new Error("密钥不存在");
    if (input.status !== undefined) k.status = input.status;
    k.updated_at = nowIso();
  },
  delete_api_key: async ({ id }: { id: string }) => {
    await delay(150);
    apiKeys = apiKeys.filter((k) => k.id !== id);
  },

  // ===== 日志 =====
  get_logs: async ({ input }: { input?: any }) => {
    await delay();
    const i = input ?? {};
    let list = [...requestLogs];
    if (i.keyword) {
      const kw = String(i.keyword).toLowerCase();
      list = list.filter(
        (l) =>
          (l.api_key_name ?? "").toLowerCase().includes(kw) ||
          (l.channel_name ?? "").toLowerCase().includes(kw) ||
          l.model.toLowerCase().includes(kw),
      );
    }
    if (i.channel_name) list = list.filter((l) => l.channel_name === i.channel_name);
    if (i.model) list = list.filter((l) => l.model === i.model);
    if (i.status_code !== undefined && i.status_code !== null && i.status_code !== 0) {
      list = list.filter((l) => l.status_code === Number(i.status_code));
    }
    if (i.start_date) list = list.filter((l) => l.created_at >= new Date(i.start_date).toISOString());
    if (i.end_date) list = list.filter((l) => l.created_at <= new Date(i.end_date).toISOString());

    list.sort((a, b) => (b.created_at > a.created_at ? 1 : -1));

    // 分页
    const page = i.page ?? 1;
    const pageSize = i.page_size ?? 20;
    const start = (page - 1) * pageSize;
    return list.slice(start, start + pageSize);
  },
  get_log: async ({ id }: { id: string }) => {
    await delay(60);
    return requestLogs.find((l) => l.id === id) ?? null;
  },
  get_log_stats: async ({ days }: { days?: number }): Promise<LogStats[]> => {
    await delay();
    const d = days ?? 30;
    const stats: LogStats[] = [];
    for (let i = d - 1; i >= 0; i--) {
      const date = new Date();
      date.setDate(date.getDate() - i);
      const dateKey = date.toISOString().slice(0, 10);
      const dayLogs = requestLogs.filter((l) => l.created_at.startsWith(dateKey));
      stats.push({
        date: dateKey,
        requests: dayLogs.length,
        tokens: dayLogs.reduce((sum, l) => sum + l.total_tokens, 0),
      });
    }
    return stats;
  },

  // ===== 仪表盘 =====
  get_dashboard_stats: async (): Promise<DashboardStats> => {
    await delay();
    const today = new Date().toISOString().slice(0, 10);
    const todayLogs = requestLogs.filter((l) => l.created_at.startsWith(today));
    const avgLatency =
      requestLogs.length > 0
        ? Math.round(
            requestLogs.reduce((s, l) => s + l.duration_ms, 0) / requestLogs.length,
          )
        : 0;
    return {
      today_requests: todayLogs.length,
      today_total_tokens: todayLogs.reduce((s, l) => s + l.total_tokens, 0),
      total_channels: channels.length,
      active_channels: channels.filter((c) => c.status === 1).length,
      total_api_keys: apiKeys.length,
      total_requests: requestLogs.length,
      total_tokens: requestLogs.reduce((s, l) => s + l.total_tokens, 0),
      avg_latency_ms: avgLatency,
    };
  },

  // ===== 设置 =====
  get_settings: async () => {
    await delay(50);
    return { ...settings };
  },
  save_settings: async ({ settings: s }: { settings: Settings }) => {
    await delay(100);
    settings = { ...s };
  },
};

// ---------- 工具函数 ----------

const generateApiKey = (): string => {
  const chars = "abcdef0123456789";
  let hex = "";
  for (let i = 0; i < 32; i++) {
    hex += chars[Math.floor(Math.random() * chars.length)];
  }
  return `sk-yeapi-${hex}`;
};

// 导出：允许外部重新播种（用于测试）
export const __resetMockData = () => {
  channels = [...seedChannels];
  apiKeys = [...seedApiKeys];
  requestLogs = buildSeedLogs();
  settings = { ...DEFAULT_SETTINGS };
};
