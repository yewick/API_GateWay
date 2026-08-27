import { useEffect, useState } from "react";
import { Search, X } from "lucide-react";
import type { LogFilters } from "../../hooks/useLogs";
import { useChannels } from "../../hooks/useChannels";
import { Button } from "../ui/Button";

interface LogFiltersProps {
  value: LogFilters;
  onChange: (filters: LogFilters) => void;
}

const STATUS_OPTIONS = [
  { value: "0", label: "全部状态" },
  { value: "200", label: "200 成功" },
  { value: "400", label: "400 客户端错误" },
  { value: "401", label: "401 未授权" },
  { value: "403", label: "403 禁止访问" },
  { value: "429", label: "429 限流" },
  { value: "500", label: "500 服务端错误" },
];

const RISK_LEVEL_OPTIONS = [
  { value: "clean", label: "clean（干净）" },
  { value: "info", label: "info（信息）" },
  { value: "low", label: "low（低）" },
  { value: "medium", label: "medium（中）" },
  { value: "high", label: "high（高）" },
  { value: "critical", label: "critical（严重）" },
];

const ACTION_OPTIONS = [
  { value: "allow", label: "allow" },
  { value: "warn", label: "warn" },
  { value: "redact", label: "redact" },
  { value: "confirm", label: "confirm" },
  { value: "block", label: "block" },
];

const MODE_OPTIONS = [
  { value: "chat", label: "chat（OpenAI 对话）" },
  { value: "messages", label: "messages（Anthropic）" },
  { value: "responses", label: "responses（Responses API）" },
  { value: "embedding", label: "embedding（向量化）" },
];

export function LogFiltersBar({ value, onChange }: LogFiltersProps) {
  const { data: channels } = useChannels();
  const [keyword, setKeyword] = useState(value.keyword ?? "");

  // 关键词防抖
  useEffect(() => {
    const t = setTimeout(() => {
      if (keyword !== (value.keyword ?? "")) {
        onChange({ ...value, keyword: keyword || undefined, page: 1 });
      }
    }, 400);
    return () => clearTimeout(t);
  }, [keyword]); // eslint-disable-line react-hooks/exhaustive-deps

  // 从渠道列表推导模型列表
  const models = Array.from(
    new Set((channels ?? []).flatMap((c) => c.models ?? [])),
  ).sort();

  const hasFilters = !!(
    value.keyword ||
    value.channel_name ||
    value.model ||
    value.mode ||
    value.status_code ||
    value.risk_level ||
    value.security_action ||
    value.finding_rule ||
    value.start_date ||
    value.end_date
  );

  const clearAll = () => {
    setKeyword("");
    onChange({ page: 1, page_size: value.page_size });
  };

  const update = (partial: Partial<LogFilters>) =>
    onChange({ ...value, ...partial, page: 1 });

  return (
    <div className="flex flex-wrap items-center gap-3 p-4 bg-bg-secondary border border-border-primary rounded-xl mb-4">
      {/* 关键词搜索 */}
      <div className="relative flex-1 min-w-[200px]">
        <Search
          size={16}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted"
        />
        <input
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder="搜索密钥名称、渠道、模型..."
          className="w-full pl-9 pr-8 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary placeholder-text-muted outline-none focus:border-accent transition-colors"
        />
        {keyword && (
          <button
            onClick={() => setKeyword("")}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary"
            aria-label="清除搜索"
          >
            <X size={14} />
          </button>
        )}
      </div>

      {/* 渠道筛选 */}
      <select
        value={value.channel_name ?? ""}
        onChange={(e) =>
          update({ channel_name: e.target.value || undefined })
        }
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        <option value="">全部渠道</option>
        {(channels ?? []).map((c) => (
          <option key={c.id} value={c.name}>
            {c.name}
          </option>
        ))}
      </select>

      {/* 模型筛选 */}
      <select
        value={value.model ?? ""}
        onChange={(e) => update({ model: e.target.value || undefined })}
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        <option value="">全部模型</option>
        {models.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))}
      </select>

      {/* 状态码筛选 */}
      <select
        value={String(value.status_code ?? 0)}
        onChange={(e) =>
          update({ status_code: e.target.value ? Number(e.target.value) : undefined })
        }
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        {STATUS_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* 协议筛选 */}
      <select
        value={value.mode ?? ""}
        onChange={(e) => update({ mode: e.target.value || undefined })}
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        <option value="">全部协议</option>
        {MODE_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* 风险等级筛选 */}
      <select
        value={value.risk_level ?? ""}
        onChange={(e) =>
          update({ risk_level: e.target.value || undefined })
        }
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        <option value="">全部风险等级</option>
        {RISK_LEVEL_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* 安全动作筛选 */}
      <select
        value={value.security_action ?? ""}
        onChange={(e) =>
          update({ security_action: e.target.value || undefined })
        }
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
      >
        <option value="">全部安全动作</option>
        {ACTION_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* 自定义规则命中 */}
      <label className="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
        <input
          type="checkbox"
          className="w-4 h-4 accent-accent cursor-pointer"
          checked={value.finding_rule === "custom."}
          onChange={(e) =>
            update({ finding_rule: e.target.checked ? "custom." : undefined })
          }
        />
        自定义规则命中
      </label>

      {/* 日期范围 */}
      <input
        type="date"
        value={value.start_date ?? ""}
        onChange={(e) => update({ start_date: e.target.value || undefined })}
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
        title="开始日期"
      />
      <span className="text-text-muted text-xs">至</span>
      <input
        type="date"
        value={value.end_date ?? ""}
        onChange={(e) => update({ end_date: e.target.value || undefined })}
        className="px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
        title="结束日期"
      />

      {hasFilters && (
        <Button variant="ghost" size="sm" onClick={clearAll}>
          <X size={14} />
          清除筛选
        </Button>
      )}
    </div>
  );
}
