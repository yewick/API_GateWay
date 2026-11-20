import {
  AlertTriangle,
  Bot,
  Copy,
  FolderOpen,
  Monitor,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Terminal,
  Unplug,
  Wifi,
} from "lucide-react";
import type { ReactNode } from "react";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { DropdownMenu } from "../ui/DropdownMenu";
import { highestSeverity, type TargetView } from "../../lib/clientAccess";
import type { ClientTestResult } from "../../types";

interface ClientTargetCardProps {
  view: TargetView;
  testing?: boolean;
  testResult?: ClientTestResult | null;
  onEdit: () => void;
  onTest: () => void;
  onDisconnect: () => void;
  onOpenDir: () => void;
  onCopyGateway: () => void;
  onRepair: () => void;
}

const icons: Record<TargetView["id"], ReactNode> = {
  codex: <Terminal size={19} />,
  "claude-cli": <Bot size={19} />,
  "claude-desktop": <Monitor size={19} />,
};

const toneClasses: Record<TargetView["state"], string> = {
  applied: "border-accent/40 bg-accent/[0.04]",
  conflict: "border-danger/40 bg-danger/[0.04]",
  drift: "border-warning/40 bg-warning/[0.04]",
  not_applied: "border-border-primary bg-bg-secondary",
  not_installed: "border-border-primary bg-bg-secondary",
};

const severityText = {
  danger: "text-danger",
  warning: "text-warning",
  info: "text-info",
};

function IconAction({
  label,
  onClick,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="p-1.5 rounded-md text-text-secondary hover:text-text-primary hover:bg-bg-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
    >
      {children}
    </button>
  );
}

export function ClientTargetCard({
  view,
  testing = false,
  testResult,
  onEdit,
  onTest,
  onDisconnect,
  onOpenDir,
  onCopyGateway,
  onRepair,
}: ClientTargetCardProps) {
  const severity = view.issues.length > 0 ? highestSeverity(view.issues) : null;
  const canRepair = view.issues.some((issue) => issue.auto_fixable);
  const detail = view.mapping
    ? `模型映射 ${view.mapping}`
    : view.model
      ? `模型 ${view.model}`
      : null;

  return (
    <div
      className={`group flex items-start gap-3 rounded-xl border px-4 py-3.5 transition-colors hover:shadow-sm ${toneClasses[view.state]}`}
    >
      <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-bg-tertiary text-text-secondary">
        {icons[view.id]}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-sm font-semibold text-text-primary">
            {view.name}
          </span>
          {view.chips.map((chip) => (
            <Badge key={chip.label} variant={chip.tone}>
              {chip.label}
            </Badge>
          ))}
        </div>

        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-text-muted">
          <span className="mono truncate max-w-full" title={view.configPath}>
            {view.configPath}
          </span>
          {detail && <span>· {detail}</span>}
          {view.keyHint && <span>· Key {view.keyHint}</span>}
        </div>

        {testResult && (
          <div
            className={`mt-1.5 text-xs ${
              testResult.ok ? "text-success" : "text-danger"
            }`}
          >
            {testResult.ok
              ? `连接正常 · ${testResult.latency_ms}ms`
              : testResult.message}
          </div>
        )}

        {view.issues.length > 0 && severity && (
          <div
            className={`mt-1.5 flex items-start gap-1 text-xs ${severityText[severity]}`}
            title={view.issues.map((issue) => issue.message).join("\n")}
          >
            <AlertTriangle size={12} className="mt-0.5 shrink-0" />
            <span className="line-clamp-1">{view.issues[0].message}</span>
          </div>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
        {view.applied ? (
          <span className="flex items-center gap-1.5 whitespace-nowrap px-1 text-xs font-medium text-success">
            <span className="h-1.5 w-1.5 rounded-full bg-success" />
            使用中
          </span>
        ) : (
          <Button
            size="sm"
            onClick={onEdit}
            disabled={view.state === "not_installed"}
            title={
              view.state === "not_installed"
                ? "未检测到客户端，请先安装或开启第三方推理"
                : undefined
            }
          >
            {view.state === "conflict" ? "接管" : "接入"}
          </Button>
        )}

        {view.applied && (
          <IconAction label="测试连接" onClick={onTest} disabled={testing}>
            <Wifi size={15} className={testing ? "animate-pulse" : ""} />
          </IconAction>
        )}
        <IconAction label="编辑接入" onClick={onEdit}>
          <Pencil size={15} />
        </IconAction>
        <DropdownMenu
          trigger={<MoreHorizontal size={16} />}
          items={[
            {
              label: "打开配置目录",
              icon: <FolderOpen size={14} />,
              onSelect: onOpenDir,
            },
            {
              label: "复制网关地址",
              icon: <Copy size={14} />,
              onSelect: onCopyGateway,
              disabled: !view.gatewayUrl,
            },
            ...(view.state === "conflict"
              ? [
                  {
                    label: "重新接管…",
                    icon: <RefreshCw size={14} />,
                    onSelect: onEdit,
                  },
                ]
              : []),
            ...(canRepair
              ? [
                  {
                    label: "立即修复",
                    icon: <RefreshCw size={14} />,
                    onSelect: onRepair,
                  },
                ]
              : []),
            ...(view.applied
              ? [
                  {
                    label: "断开接入",
                    icon: <Unplug size={14} />,
                    danger: true,
                    onSelect: onDisconnect,
                  },
                ]
              : []),
          ]}
        />
      </div>
    </div>
  );
}
