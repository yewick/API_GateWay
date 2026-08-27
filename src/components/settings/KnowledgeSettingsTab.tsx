import { BookOpen } from "lucide-react";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { useSettingsStore } from "../../stores/settingsStore";

// 常见 embedding 模型预设（模型名以渠道实际支持为准）
const EMBEDDING_MODEL_PRESETS = [
  { value: "embedding-3", label: "embedding-3（智谱）" },
  { value: "embedding-2", label: "embedding-2（智谱）" },
  { value: "text-embedding-3-small", label: "text-embedding-3-small（OpenAI）" },
  { value: "text-embedding-3-large", label: "text-embedding-3-large（OpenAI）" },
  { value: "text-embedding-ada-002", label: "text-embedding-ada-002（OpenAI）" },
];

// PDF 解析后端（与后端 PdfBackend 对应）
const PDF_BACKEND_OPTIONS = [
  { value: "native", label: "原生解析（unpdf，零依赖）" },
  { value: "pymupdf", label: "PyMuPDF（保留标题/表格结构）" },
  { value: "mineru", label: "MinerU 云解析（扫描件/复杂表格）" },
];

// MinerU 调用模式
const MINERU_MODE_OPTIONS = [
  { value: "agent", label: "Agent（轻量，免 token，限流）" },
  { value: "precise", label: "Precise（需 token，大文件/多页）" },
];

/** 知识库相关配置：默认 embedding 模型（todo §8.1）+ PDF 解析方式与 MinerU（todo §8.2）。 */
export function KnowledgeSettingsTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  // 当前值不在预设中时（如自定义模型名），追加为选项避免显示空白
  const modelOptions =
    settings.default_embedding_model &&
    !EMBEDDING_MODEL_PRESETS.some((o) => o.value === settings.default_embedding_model)
      ? [
          { value: settings.default_embedding_model, label: settings.default_embedding_model },
          ...EMBEDDING_MODEL_PRESETS,
        ]
      : EMBEDDING_MODEL_PRESETS;

  const isMinerU = settings.pdf_backend === "mineru";

  return (
    <div className="space-y-8 max-w-xl">
      <div>
        <div className="flex items-center gap-2 mb-1">
          <BookOpen size={14} className="text-text-muted" />
          <h4 className="text-sm font-semibold text-text-primary">知识库</h4>
        </div>
        <p className="text-xs text-text-muted mb-4">
          默认 Embedding 模型与 PDF 解析配置。保存后立即生效。
        </p>
      </div>

      {/* 默认 Embedding 模型（todo §8.1） */}
      <div>
        <Select
          label="默认 Embedding 模型"
          options={modelOptions}
          value={settings.default_embedding_model}
          onChange={(e) => updateSettings({ default_embedding_model: e.target.value })}
          hint="知识库未单独配置模型时，检索/入库使用该默认模型（模型名以渠道实际支持为准）"
        />
      </div>

      {/* PDF 解析方式（todo §8.2） */}
      <div className="pt-2 border-t border-border-primary">
        <Select
          label="PDF 解析方式"
          options={PDF_BACKEND_OPTIONS}
          value={settings.pdf_backend}
          onChange={(e) => updateSettings({ pdf_backend: e.target.value })}
          hint="上传 PDF 时使用的解析后端：原生（unpdf）、PyMuPDF（需本地 Python）、MinerU（云服务）"
        />
      </div>

      {/* MinerU 解析（仅选择 MinerU 后端时显示） */}
      {isMinerU && (
        <div className="space-y-4 pt-2 border-t border-border-primary">
          <div>
            <h4 className="text-sm font-semibold text-text-primary mb-1">MinerU PDF 解析</h4>
            <p className="text-xs text-text-muted">
              面向扫描件/复杂表格的云解析。环境变量 YEAPI_MINERU_* 仍可作临时覆盖。
            </p>
          </div>
          <Select
            label="解析模式"
            options={MINERU_MODE_OPTIONS}
            value={settings.mineru_mode}
            onChange={(e) => updateSettings({ mineru_mode: e.target.value })}
            hint="Agent 免 token 但有限流；Precise 需配置 token，支持更大文件与页数"
          />
          <Input
            label="Token"
            value={settings.mineru_token}
            onChange={(e) => updateSettings({ mineru_token: e.target.value })}
            placeholder={
              settings.mineru_mode === "precise" ? "Precise 模式需填写 token" : "留空使用 Agent 轻量 API"
            }
            hint="mineru.net 的 API token，Precise 模式必填"
          />
          <div className="grid grid-cols-2 gap-4">
            <Input
              label="Base URL"
              value={settings.mineru_base_url}
              onChange={(e) => updateSettings({ mineru_base_url: e.target.value })}
              placeholder="https://mineru.net"
            />
            <Input
              label="Model Version"
              value={settings.mineru_model}
              onChange={(e) => updateSettings({ mineru_model: e.target.value })}
              placeholder="pipeline"
              hint="Precise API 解析模型（pipeline / vlm）"
            />
          </div>
        </div>
      )}
    </div>
  );
}
