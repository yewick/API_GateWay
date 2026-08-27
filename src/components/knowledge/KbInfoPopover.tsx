import { useEffect, useRef, useState } from "react";
import { Info, Database, Layers, Coins, Cpu, Trash2 } from "lucide-react";
import { Badge } from "../ui/Badge";
import { Toggle } from "../ui/Toggle";
import { Button } from "../ui/Button";
import { indexStatus } from "../../lib/knowledge";
import { formatNumber } from "../../lib/constants";
import type { KbKnowledgeBase } from "../../types";

interface KbInfoPopoverProps {
  kb: KbKnowledgeBase;
  onToggleMcp: (kb: KbKnowledgeBase) => void;
  onDelete: (kb: KbKnowledgeBase) => void;
}

/** 知识库信息气泡：点击标题旁的小按钮弹出，展示统计、索引状态、MCP 开关与删除入口。 */
export function KbInfoPopover({ kb, onToggleMcp, onDelete }: KbInfoPopoverProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocMouseDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocMouseDown);
    return () => document.removeEventListener("mousedown", onDocMouseDown);
  }, [open]);

  const idx = indexStatus(kb.index_status);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="p-2 rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
        aria-label="知识库信息"
        title="知识库信息"
      >
        <Info size={16} />
      </button>

      {open && (
        <div className="absolute right-0 top-full mt-2 w-72 z-50 rounded-xl bg-bg-secondary border border-border-primary shadow-xl p-4 space-y-4">
          <div>
            <p className="text-xs text-text-muted mb-2">知识库信息</p>
            <div className="grid grid-cols-2 gap-2">
              <Stat icon={<Database size={14} />} label="文档" value={formatNumber(kb.doc_count)} />
              <Stat icon={<Layers size={14} />} label="切片" value={formatNumber(kb.chunk_count)} />
              <Stat icon={<Coins size={14} />} label="Tokens" value={formatNumber(kb.total_tokens)} />
              <Stat
                icon={<Cpu size={14} />}
                label="Embedding"
                value={kb.embedding_dim > 0 ? `${kb.embedding_dim} 维` : "—"}
              />
            </div>
          </div>

          <div className="flex items-center gap-2">
            <span className="text-xs text-text-muted">索引状态</span>
            <Badge variant={idx.variant}>{idx.label}</Badge>
          </div>

          <div className="pt-3 border-t border-border-primary">
            <Toggle
              checked={kb.mcp_enabled === 1}
              onChange={() => onToggleMcp(kb)}
              label="MCP 服务"
              description="向 MCP 客户端暴露检索工具"
            />
          </div>

          <div className="pt-3 border-t border-border-primary">
            <Button
              variant="danger"
              size="sm"
              className="w-full"
              onClick={() => {
                setOpen(false);
                onDelete(kb);
              }}
            >
              <Trash2 size={14} />
              删除知识库
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function Stat({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-2 p-2 rounded-lg bg-bg-tertiary border border-border-primary">
      <div className="text-text-muted flex-shrink-0">{icon}</div>
      <div className="min-w-0">
        <div className="text-[10px] text-text-muted">{label}</div>
        <div className="text-xs font-semibold text-text-primary tabular truncate">{value}</div>
      </div>
    </div>
  );
}
