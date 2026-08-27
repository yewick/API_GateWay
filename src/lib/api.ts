import { invoke } from "./invoke-adapter";
import type { Channel, CreateChannelInput, UpdateChannelInput, TestChannelResult,
  ApiKey, CreateApiKeyInput, RequestLog, LogStats, LogModeStats, DashboardStats, Settings,
  BuiltinRule, UpdateBuiltinRuleInput, CustomRule, CreateCustomRuleInput,
  SecurityFinding, KbKnowledgeBase, KbDocument, KbSource, KbStats, KbConversation,
  SearchResult, RagAnswer, CreateKbInput, UpdateKbInput, ImportSourceInput,
  IndexSummary, DocumentContent, UploadDocumentResult, ConversationMessage } from "../types";

// 渠道管理 API
export const channelApi = {
  getAll: () => invoke<Channel[]>("get_channels"),
  get: (id: string) => invoke<Channel>("get_channel", { id }),
  create: (input: CreateChannelInput) => invoke<Channel>("create_channel", { input }),
  update: (input: UpdateChannelInput) => invoke<Channel>("update_channel", { input }),
  toggle: (id: string, status: number) => invoke<void>("toggle_channel", { id, status }),
  delete: (id: string) => invoke<void>("delete_channel", { id }),
  test: (id: string) => invoke<TestChannelResult>("test_channel", { id }),
  reorder: (ids: string[]) => invoke<void>("reorder_channels", { ids }),
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
  getModeStats: (days?: number) => invoke<LogModeStats[]>("get_mode_stats", { days }),
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
  restartServer: () => invoke<void>("restart_server"),
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

// 导出 API（系统「另存为」对话框选路径后，由 Rust 侧落盘）
export const exportApi = {
  writeTextFile: (path: string, content: string) =>
    invoke<void>("write_text_file", { path, content }),
};

// 知识库 API
export const knowledgeApi = {
  // 知识库 CRUD
  list: () => invoke<KbKnowledgeBase[]>("get_knowledge_bases"),
  get: (id: string) => invoke<KbKnowledgeBase>("get_knowledge_base", { id }),
  create: (input: CreateKbInput) => invoke<KbKnowledgeBase>("create_knowledge_base", { input }),
  update: (id: string, input: UpdateKbInput) =>
    invoke<KbKnowledgeBase>("update_knowledge_base", { id, input }),
  remove: (id: string) => invoke<void>("delete_knowledge_base", { id }),
  // 文档
  listDocuments: (kbId: string) => invoke<KbDocument[]>("list_kb_documents", { kbId }),
  getDocument: (kbId: string, docId: string) =>
    invoke<KbDocument>("get_kb_document", { kbId, docId }),
  getDocumentContent: (kbId: string, docId: string) =>
    invoke<DocumentContent>("get_kb_document_content", { kbId, docId }),
  uploadDocument: (kbId: string, path: string) =>
    invoke<UploadDocumentResult>("upload_kb_document", { kbId, path }),
  ingestDocument: (kbId: string, docId: string) =>
    invoke<{ doc_id: string; chunk_count: number; token_count: number; embedding_dim: number | null }>(
      "ingest_kb_document",
      { kbId, docId },
    ),
  deleteDocument: (kbId: string, docId: string) =>
    invoke<void>("delete_kb_document", { kbId, docId }),
  // 统计 / 索引
  getStats: (kbId: string) => invoke<KbStats>("get_kb_stats", { kbId }),
  buildIndex: (kbId: string) => invoke<IndexSummary>("build_kb_index", { kbId }),
  getIndex: (kbId: string) => invoke<IndexSummary>("get_kb_index", { kbId }),
  // 检索 / 问答
  search: (kbId: string, query: string, topK?: number, symbolKind?: string) =>
    invoke<SearchResult[]>("search_kb", { kbId, query, topK, symbolKind }),
  ask: (
    kbId: string,
    question: string,
    options?: { model?: string; topK?: number; history?: ConversationMessage[]; apiKeyId?: string },
  ) =>
    invoke<RagAnswer>("ask_knowledge_base", {
      kbId,
      question,
      model: options?.model ?? null,
      topK: options?.topK ?? null,
      history: options?.history ?? null,
      apiKeyId: options?.apiKeyId ?? null,
    }),
  // 对话
  getConversations: (kbId: string) => invoke<KbConversation[]>("get_kb_conversations", { kbId }),
  clearConversations: (kbId: string) => invoke<void>("delete_kb_conversations", { kbId }),
  // 多源导入
  importSource: (kbId: string, input: ImportSourceInput) =>
    invoke<{ source_id: string; file_count: number; status: string; error: string | null }>(
      "import_kb_source",
      { kbId, input },
    ),
  listSources: (kbId: string) => invoke<KbSource[]>("list_kb_sources", { kbId }),
  deleteSource: (kbId: string, sourceId: string) =>
    invoke<void>("delete_kb_source", { kbId, sourceId }),
};