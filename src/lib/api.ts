import { invoke } from "./invoke-adapter";
import type { Channel, CreateChannelInput, UpdateChannelInput, TestChannelResult,
  ApiKey, CreateApiKeyInput, RequestLog, LogStats, DashboardStats, Settings,
  BuiltinRule, UpdateBuiltinRuleInput, CustomRule, CreateCustomRuleInput,
  SecurityFinding } from "../types";

// 渠道管理 API
export const channelApi = {
  getAll: () => invoke<Channel[]>("get_channels"),
  get: (id: string) => invoke<Channel>("get_channel", { id }),
  create: (input: CreateChannelInput) => invoke<Channel>("create_channel", { input }),
  update: (input: UpdateChannelInput) => invoke<Channel>("update_channel", { input }),
  toggle: (id: string, status: number) => invoke<void>("toggle_channel", { id, status }),
  delete: (id: string) => invoke<void>("delete_channel", { id }),
  test: (id: string) => invoke<TestChannelResult>("test_channel", { id }),
};

// 密钥管理 API
export const apiKeyApi = {
  getAll: () => invoke<ApiKey[]>("get_api_keys"),
  create: (input: CreateApiKeyInput) => invoke<ApiKey>("create_api_key", { input }),
  update: (id: string, status?: number) => invoke<void>("update_api_key", { input: { id, status } }),
  delete: (id: string) => invoke<void>("delete_api_key", { id }),
};

interface GetLogsInput {
  keyword?: string;
  api_key_name?: string;
  channel_name?: string;
  model?: string;
  mode?: string;
  status_code?: number;
  is_stream?: boolean;
  is_retry?: boolean;
  risk_level?: string;
  security_action?: string;
  finding_rule?: string;
  start_date?: string;
  end_date?: string;
  page?: number;
  page_size?: number;
}

// 日志 API
export const logApi = {
  getAll: (input?: GetLogsInput) => invoke<RequestLog[]>("get_logs", { input: input || {} }),
  get: (id: string) => invoke<RequestLog>("get_log", { id }),
  delete: (id: string) => invoke<void>("delete_log", { id }),
  getStats: (days?: number) => invoke<LogStats[]>("get_log_stats", { days }),
  getFindings: (logId: string) => invoke<SecurityFinding[]>("get_log_findings", { logId }),
};

// 仪表盘 API
export const statsApi = {
  getDashboard: () => invoke<DashboardStats>("get_dashboard_stats"),
};

// 设置 API
export const settingsApi = {
  get: () => invoke<Settings>("get_settings"),
  save: (settings: Settings) => invoke<void>("save_settings", { settings }),
};

// 安全规则 API
export const securityApi = {
  getBuiltinRules: () => invoke<BuiltinRule[]>("get_builtin_security_rules"),
  updateBuiltinRule: (id: string, input: UpdateBuiltinRuleInput) =>
    invoke<void>("update_builtin_security_rule", { id, input }),
  resetBuiltinRules: () => invoke<void>("reset_builtin_security_rules"),
  getCustomRules: () => invoke<CustomRule[]>("get_custom_security_rules"),
  createCustomRule: (input: CreateCustomRuleInput) =>
    invoke<CustomRule>("create_custom_security_rule", { input }),
  toggleCustomRule: (id: string, enabled: boolean) =>
    invoke<void>("toggle_custom_security_rule", { id, enabled }),
  deleteCustomRule: (id: string) =>
    invoke<void>("delete_custom_security_rule", { id }),
};

// 测试台 API
export const testApi = {
  send: (input: { host: string; api_key: string; model: string; content: string }) =>
    invoke<{ status: number; body: unknown }>("send_test_request", { input }),
};