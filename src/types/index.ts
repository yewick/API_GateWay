export interface Channel {
  id: string;
  name: string;
  type: string;
  base_url: string;
  api_key: string;
  models: string[];
  status: number;
  priority: number;
  weight: number;
  config: Record<string, unknown>;
  model_mapping: Record<string, string>;
  created_at: string;
  updated_at: string;
  last_test_at: string | null;
  last_test_ok: number | null;
}

export interface ApiKey {
  id: string;
  name: string;
  key: string;
  status: number;
  allowed_models: string[];
  allowed_channels: string[];
  quota_limit: number;
  quota_used: number;
  expires_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface RequestLog {
  id: string;
  seq: number | null;
  api_key_name: string | null;
  channel_name: string | null;
  model: string;
  upstream_model: string | null;
  mode: string;
  status_code: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  duration_ms: number;
  error_message: string | null;
  is_stream: boolean;
  is_retry: boolean;
  created_at: string;
  request_body: string | null;
  forward_body: string | null;
  response_choices: string | null;
  trace_id: string | null;
  risk_level: string;
  risk_score: number;
  risk_summary: string | null;
  security_action: string;
  sanitized: boolean;
  blocked_reason: string | null;
}

export interface DashboardStats {
  today_requests: number;
  today_total_tokens: number;
  active_channels: number;
  avg_latency_ms: number;
  total_channels: number;
  total_api_keys: number;
  total_requests: number;
  total_tokens: number;
}

export interface Settings {
  server_port: number;
  server_host: string;
  ui_theme: string;
  ui_language: string;
  minimize_to_tray: boolean;
  close_to_tray: boolean;
  auto_start: boolean;
  retry_enabled: boolean;
  retry_times: number;
  default_embedding_model: string;
  mineru_token: string;
  mineru_base_url: string;
  mineru_model: string;
  security_enabled: boolean;
  security_mode: string;
  security_scan_request: boolean;
  security_scan_unicode: boolean;
  security_scan_tools: boolean;
  security_scan_network: boolean;
  security_scan_response: boolean;
  security_redact_secrets: boolean;
  security_block_on_critical: boolean;
}

export interface CreateChannelInput {
  name: string;
  type: string;
  base_url: string;
  api_key: string;
  models?: string[];
  priority?: number;
  weight?: number;
  config?: Record<string, unknown>;
  model_mapping?: Record<string, string>;
}

export interface UpdateChannelInput {
  id: string;
  name?: string;
  type?: string;
  base_url?: string;
  api_key?: string;
  models?: string[];
  status?: number;
  priority?: number;
  weight?: number;
  config?: Record<string, unknown>;
  model_mapping?: Record<string, string>;
}

export interface TestChannelResult {
  success: boolean;
  latency_ms: number;
  error_message?: string;
}

export interface CreateApiKeyInput {
  name: string;
  allowed_models?: string[];
  allowed_channels?: string[];
  quota_limit?: number;
  expires_at?: string | null;
}

export interface LogStats {
  date: string;
  requests: number;
  tokens: number;
}

export interface LogModeStats {
  date: string;
  mode: string;
  requests: number;
  tokens: number;
}

// ---------- 安全规则 ----------

export interface BuiltinRule {
  id: string;
  rule_id: string;
  category: string;
  severity: string;
  title: string;
  description: string | null;
  toggle_key: string | null;
  enabled: number;
  created_at: string;
}

export interface UpdateBuiltinRuleInput {
  enabled?: boolean;
  severity?: string;
  title?: string;
  description?: string;
}

export interface CustomRule {
  id: string;
  rule_type: string;
  category: string;
  pattern: string;
  severity: string;
  action: string | null;
  enabled: number;
  description: string | null;
  created_at: string;
}

export interface CreateCustomRuleInput {
  rule_type: string;
  category: string;
  pattern: string;
  severity?: string;
  action?: string;
  description?: string;
}

export interface SecurityFinding {
  id: number;
  log_id: string;
  rule: string;
  severity: string;
  detail: string | null;
  action: string;
  created_at: string;
}

// ---------- 知识库 ----------

export interface KbKnowledgeBase {
  id: string;
  name: string;
  description: string | null;
  status: number;
  doc_count: number;
  chunk_count: number;
  total_tokens: number;
  embedding_model: string | null;
  embedding_channel_id: string | null;
  mcp_enabled: number;
  chunk_size: number;
  chunk_overlap: number;
  excluded_dirs: string;
  excluded_files: string;
  included_files: string;
  embedding_dim: number;
  index_status: string;
  created_at: string;
  updated_at: string;
}

export interface KbDocument {
  id: string;
  kb_id: string;
  filename: string;
  file_path: string | null;
  file_type: string;
  file_size: number;
  content_hash: string;
  content: string;
  chunk_count: number;
  token_count: number;
  status: string;
  error_message: string | null;
  source_type: string;
  source_url: string | null;
  source_path: string | null;
  doc_meta: string;
  created_at: string;
  updated_at: string;
}

export interface KbSource {
  id: string;
  kb_id: string;
  source_type: string;
  source_url: string | null;
  source_path: string | null;
  branch: string | null;
  status: string;
  file_count: number;
  error: string | null;
  created_at: string;
  updated_at: string;
}

export interface KbStats {
  doc_count: number;
  chunk_count: number;
  total_tokens: number;
}

export interface KbConversation {
  id: string;
  kb_id: string;
  role: string;
  content: string;
  sources: string | null;
  model: string | null;
  tokens_used: number;
  created_at: string;
}

export interface SearchResult {
  chunk_id: string;
  doc_id: string;
  filename: string;
  content: string;
  score: number;
  metadata: Record<string, unknown>;
}

export interface RagUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface RagAnswer {
  answer: string;
  sources: SearchResult[];
  usage: RagUsage | null;
}

export interface ConversationMessage {
  role: string;
  content: string;
}

export interface CreateKbInput {
  name: string;
  description?: string | null;
  embedding_model?: string | null;
  embedding_channel_id?: string | null;
}

export interface UpdateKbInput {
  name?: string | null;
  description?: string | null;
  embedding_model?: string | null;
  embedding_channel_id?: string | null;
  status?: number;
  mcp_enabled?: number;
  chunk_size?: number;
  chunk_overlap?: number;
  excluded_dirs?: string;
  excluded_files?: string;
  included_files?: string;
}

export interface ImportSourceInput {
  source_type: string;
  repo_url?: string | null;
  branch?: string | null;
  token?: string | null;
  url?: string | null;
  dir_path?: string | null;
}

export interface IndexSummary {
  kb_id: string;
  status: string;
  index_type: string;
  chunk_count: number;
  embedding_dim: number;
  index_path: string | null;
  skipped: number;
}

export interface DocumentContent {
  content: string;
  file_type: string;
}

/** 切片查看项（文档查看器用，不含向量） */
export interface KbChunkView {
  chunk_index: number;
  content: string;
  token_count: number;
  symbol_name: string | null;
  symbol_kind: string | null;
  metadata: string;
}

export interface UploadDocumentResult {
  document: KbDocument;
  task_id: string;
  duplicate: boolean;
}