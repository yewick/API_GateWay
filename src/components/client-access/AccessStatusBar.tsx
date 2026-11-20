import { RefreshCw } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "../ui/Button";
import { serverApi } from "../../lib/api";
import { serverStatusKeys, useServerStatus } from "../../hooks/useServerStatus";
import { toast } from "../../lib/toast";
import type { ClientSetupStatus } from "../../types";
import type { TargetView } from "../../lib/clientAccess";

interface AccessStatusBarProps {
  views: TargetView[];
  claude?: ClientSetupStatus;
}

/// 顶部状态条：当前接入目标 + 路由服务运行状态。
export function AccessStatusBar({ views, claude }: AccessStatusBarProps) {
  const { data: server } = useServerStatus();
  const queryClient = useQueryClient();
  const restart = useMutation({
    mutationFn: serverApi.restart,
    onSuccess: (next) => {
      queryClient.invalidateQueries({ queryKey: serverStatusKeys.all });
      toast.success("网关已重启", `${next.host}:${next.actual_port}`);
    },
    onError: (error) => toast.error("网关重启失败", (error as Error).message),
  });

  const applied = views.filter((view) => view.applied);
  const env = claude?.claude?.env_models;
  const mapping = env
    ? [
        env.opus ? `Opus→${env.opus}` : null,
        env.sonnet ? `Sonnet→${env.sonnet}` : null,
        env.haiku ? `Haiku→${env.haiku}` : null,
        env.fable ? `Fable→${env.fable}` : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "";

  const gatewayState = server?.error
    ? { dot: "bg-danger", text: "路由服务 · 绑定失败", tone: "text-danger" }
    : server?.running
      ? { dot: "bg-success", text: "路由服务 · 运行中", tone: "text-success" }
      : { dot: "bg-text-muted", text: "路由服务 · 未运行", tone: "text-text-muted" };

  return (
    <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border-primary bg-bg-secondary px-4 py-3">
      <div className="flex min-w-0 items-center gap-2 text-xs">
        <span className="text-text-muted shrink-0">当前接入：</span>
        {applied.length === 0 ? (
          <span className="text-text-secondary">尚未接入客户端</span>
        ) : (
          <span className="min-w-0 truncate font-medium text-text-primary">
            {applied.map((view) => view.name).join(" · ")}
          </span>
        )}
        {mapping && (
          <span
            className="hidden md:inline min-w-0 truncate text-text-muted"
            title={mapping}
          >
            · 模型映射 {mapping}
          </span>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-2 text-xs">
        <span className={`flex items-center gap-1.5 ${gatewayState.tone}`}>
          <span className={`h-1.5 w-1.5 rounded-full ${gatewayState.dot}`} />
          {gatewayState.text}
        </span>
        <span className="mono text-text-muted" title={server?.error?.message}>
          {server?.running ? `${server.host}:${server.actual_port}` : server?.url}
        </span>
        {!server?.running && (
          <Button
            size="sm"
            variant="ghost"
            loading={restart.isPending}
            onClick={() => restart.mutate()}
          >
            <RefreshCw size={13} />
            {server?.error ? "重试" : "启动"}
          </Button>
        )}
      </div>
    </div>
  );
}
