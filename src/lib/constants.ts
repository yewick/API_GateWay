// ===== 应用常量定义 =====

// 渠道类型注册表（与后端 adaptor/mod.rs 的 channel_types() 对应）
export interface ChannelTypeInfo {
  value: string;
  label: string;
  category: "international" | "domestic" | "local" | "custom";
  defaultBaseUrl: string;
  defaultModels: string[];
}

export const CHANNEL_TYPES: ChannelTypeInfo[] = [
  {
    value: "openai",
    label: "OpenAI",
    category: "international",
    defaultBaseUrl: "https://api.openai.com/v1",
    defaultModels: ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o3-mini"],
  },
  {
    value: "deepseek",
    label: "DeepSeek",
    category: "international",
    defaultBaseUrl: "https://api.deepseek.com",
    defaultModels: ["deepseek-v4-flash", "deepseek-v4-pro"],
  },
  {
    value: "claude",
    label: "Anthropic Claude",
    category: "international",
    defaultBaseUrl: "https://api.anthropic.com",
    defaultModels: ["claude-sonnet-4-20250514", "claude-3-5-haiku-20241022"],
  },
  {
    value: "gemini",
    label: "Google Gemini",
    category: "international",
    defaultBaseUrl: "https://generativelanguage.googleapis.com",
    defaultModels: ["gemini-2.5-flash", "gemini-2.5-pro"],
  },
  {
    value: "qwen",
    label: "通义千问",
    category: "domestic",
    defaultBaseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    defaultModels: ["qwen-plus", "qwen-max", "qwen-turbo"],
  },
  {
    value: "zhipu",
    label: "智谱 GLM",
    category: "domestic",
    defaultBaseUrl: "https://open.bigmodel.cn/api/paas/v4",
    defaultModels: ["glm-4-plus", "glm-4-air"],
  },
  {
    value: "moonshot",
    label: "月之暗面 Kimi",
    category: "domestic",
    defaultBaseUrl: "https://api.moonshot.cn/v1",
    defaultModels: ["moonshot-v1-8k", "moonshot-v1-32k", "kimi-k2"],
  },
  {
    value: "doubao",
    label: "字节豆包",
    category: "domestic",
    defaultBaseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    defaultModels: ["doubao-pro-32k", "doubao-lite-32k"],
  },
  {
    value: "ollama",
    label: "Ollama",
    category: "local",
    defaultBaseUrl: "http://localhost:11434/v1",
    defaultModels: ["llama3.1", "qwen2.5", "mistral"],
  },
  {
    value: "custom",
    label: "自定义",
    category: "custom",
    defaultBaseUrl: "",
    defaultModels: [],
  },
];

export const getChannelType = (value: string): ChannelTypeInfo | undefined =>
  CHANNEL_TYPES.find((t) => t.value === value);

// 侧边栏导航项
export interface NavItem {
  path: string;
  label: string;
  icon: string; // lucide-react 图标名
}

export const NAV_ITEMS: NavItem[] = [
  { path: "/", label: "仪表盘", icon: "LayoutDashboard" },
  { path: "/usage", label: "用量", icon: "BarChart3" },
  { path: "/channels", label: "渠道", icon: "Network" },
  { path: "/api-keys", label: "密钥", icon: "KeyRound" },
  { path: "/logs", label: "日志", icon: "ScrollText" },
  { path: "/settings", label: "设置", icon: "Settings" },
];

// 渠道状态
export const STATUS_MAP: Record<number, { label: string; color: string }> = {
  1: { label: "启用", color: "success" },
  0: { label: "禁用", color: "neutral" },
};

// 风险等级
export const RISK_LEVELS: Record<string, { label: string; color: string }> = {
  low: { label: "低风险", color: "success" },
  medium: { label: "中风险", color: "warning" },
  high: { label: "高风险", color: "danger" },
  critical: { label: "严重风险", color: "danger" },
  none: { label: "无风险", color: "neutral" },
};

// HTTP 状态码分组
export const statusColor = (code: number): string => {
  if (code >= 200 && code < 300) return "success";
  if (code >= 300 && code < 400) return "info";
  if (code >= 400 && code < 500) return "warning";
  return "danger";
};

// 语言选项
export const LANGUAGES = [
  { value: "zh-CN", label: "简体中文" },
  { value: "en-US", label: "English" },
];

// 默认设置
export const DEFAULT_SETTINGS = {
  server_port: 8777, // 默认端口 8777
  server_host: "127.0.0.1",
  ui_theme: "dark",
  ui_language: "zh-CN",
  minimize_to_tray: true,
  close_to_tray: false,
  auto_start: false,
  retry_enabled: true,
  retry_times: 3,
  security_enabled: true,
  security_mode: "audit",
  security_scan_unicode: false,
  security_scan_tools: true,
  security_scan_network: true,
  security_scan_response: false,
  security_redact_secrets: true,
  security_block_on_critical: false,
};

// 时间格式化工具
export const formatTime = (iso: string | null | undefined): string => {
  if (!iso) return "-";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

// 数字格式化（千分位）
export const formatNumber = (n: number): string => n.toLocaleString("zh-CN");
