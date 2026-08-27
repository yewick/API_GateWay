import type { BadgeVariant } from "../components/ui/Badge";

// 文档解析/入库状态（与后端 kb_documents.status 对应）
export const DOC_STATUS_MAP: Record<string, { label: string; variant: BadgeVariant }> = {
  parsing: { label: "解析中", variant: "info" },
  processing: { label: "入库中", variant: "info" },
  awaiting_review: { label: "待入库", variant: "warning" },
  ready: { label: "已就绪", variant: "success" },
  failed: { label: "失败", variant: "danger" },
};

// 索引状态（与后端 kb.index_status / kb_index_meta.status 对应）
export const INDEX_STATUS_MAP: Record<string, { label: string; variant: BadgeVariant }> = {
  none: { label: "未构建", variant: "neutral" },
  building: { label: "构建中", variant: "info" },
  ready: { label: "就绪", variant: "success" },
  failed: { label: "失败", variant: "danger" },
};

export const docStatus = (s: string) =>
  DOC_STATUS_MAP[s] ?? { label: s || "未知", variant: "neutral" as BadgeVariant };

export const indexStatus = (s: string) =>
  INDEX_STATUS_MAP[s] ?? { label: s || "未知", variant: "neutral" as BadgeVariant };

export const formatBytes = (n: number): string => {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
};

// 可上传文档的常见扩展名（后端 parser 支持的文本/办公类）
export const DOC_EXTENSIONS = [
  "md", "txt", "pdf", "docx", "html", "htm", "csv", "json",
  "rs", "py", "ts", "tsx", "js", "jsx", "java", "go", "c", "cpp", "h",
];
