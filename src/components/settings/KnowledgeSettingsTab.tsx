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

/** 知识库相关配置：默认 embedding 模型（todo §8.1）+ MinerU PDF 解析（todo §8.2）。 */
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

  return (
    <div className="space-y-8 max-w-xl">
      <div>
        <div className="flex items-center gap-2 mb-1">
          <BookOpen size={14} className="text-text-muted" />
          <h4 className="text-sm font-semibold text-text-primary">知识库</h4>
        </div>
        <p className="text-xs text-text-muted mb-4">
          默认 Embedding 模型与 MinerU PDF 解析配置。保存后立即生效。
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

      {/* MinerU PDF 解析（todo §8.2） */}
      <div className="space-y-4 pt-2 border-t border-border-primary">
        <div>
          <h4 className="text-sm font-semibold text-text-primary mb-1">MinerU PDF 解析</h4>
          <p className="text-xs text-text-muted">
            面向扫描件/复杂表格的云解析（PDF 后端选 MinerU 时生效）。token 留空走 Agent
            轻量 API（免费限流），填写走 Precise API；环境变量 YEAPI_MINERU_* 仍可作临时覆盖。
          </p>
        </div>
        <Input
          label="Token"
          value={settings.mineru_token}
          onChange={(e) => updateSettings({ mineru_token: e.target.value })}
          placeholder="留空使用 Agent 轻量 API"
          hint="mineru.net 的 API token，填写后走 Precise API（支持更大文件与页数）"
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
    </div>
  );
}
