import type { ReactNode } from "react";
import {
  Bot,
  ChevronRight,
  Monitor,
  Terminal,
} from "lucide-react";
import { Badge } from "../ui/Badge";
import { Modal } from "../ui/Modal";
import type { TargetId, TargetView } from "../../lib/clientAccess";

interface AddTargetDialogProps {
  open: boolean;
  views: TargetView[];
  onClose: () => void;
  onPick: (target: TargetId) => void;
}

const icons: Record<TargetId, ReactNode> = {
  codex: <Terminal size={18} />,
  "claude-cli": <Bot size={18} />,
  "claude-desktop": <Monitor size={18} />,
};

export function AddTargetDialog({
  open,
  views,
  onClose,
  onPick,
}: AddTargetDialogProps) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="添加接入"
      description="选择要接入 YeAPI 的客户端；已接入的目标会打开编辑面板。"
      size="md"
    >
      <div className="space-y-2">
        {views.map((view) => (
          <button
            key={view.id}
            type="button"
            disabled={view.state === "not_installed"}
            onClick={() => onPick(view.id)}
            className="w-full flex items-center gap-3 rounded-xl border border-border-primary bg-bg-secondary px-3 py-3 text-left transition-colors hover:bg-bg-hover disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-bg-tertiary text-text-secondary">
              {icons[view.id]}
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-1.5">
                <span className="text-sm font-medium text-text-primary">
                  {view.name}
                </span>
                <Badge variant="outline">{view.stateLabel}</Badge>
              </span>
              <span className="mt-0.5 block truncate text-xs text-text-muted">
                {view.description}
              </span>
            </span>
            <ChevronRight size={16} className="shrink-0 text-text-muted" />
          </button>
        ))}
      </div>
    </Modal>
  );
}
