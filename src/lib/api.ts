import { invoke } from "./invoke-adapter";
import type { Channel, CreateChannelInput, UpdateChannelInput, TestChannelResult,
  ApiKey, CreateApiKeyInput, RequestLog, LogStats, DashboardStats, Settings } from "../types";

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