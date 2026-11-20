import { AlertTriangle, RefreshCw } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "../ui/Button";
import { serverApi } from "../../lib/api";
import { serverStatusKeys, useServerStatus } from "../../hooks/useServerStatus";
import { toast } from "../../lib/toast";

/// 网关未运行或绑定失败时的全局提示；只在 Tauri 内显示。
export function GatewayStatusBanner() {
  const { data: status, isLoading } = useServerStatus();
  const queryClient = useQueryClient();
  const restart = useMutation({
    mutationFn: serverApi.restart,
    onSuccess: (next) => {
      queryClient.invalidateQueries({ queryKey: serverStatusKeys.all });
      toast.success("网关已重启", `${next.host}:${next.actual_port}`);
    },
    onError: (error) => toast.error("网关重启失败", (error as Error).message),
  });

  if (
    typeof window === "undefined" ||
    !("__TAURI_INTERNALS__" in window)
  ) {
    return null;
  }
  if (isLoading || !status) return null;
  if (status.running && !status.error) return null;

  const error = status.error;
  const tone = error
    ? "border-danger/30 bg-danger/10 text-danger"
    : "border-warning/30 bg-warning/10 text-warning";

  return (
    <div
      className={`mb-5 flex items-start gap-3 rounded-xl border px-4 py-3 text-sm ${tone}`}
    >
      <AlertTriangle size={18} className="mt-0.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <div className="font-medium">
          {error ? "本地网关启动失败" : "本地网关未运行"}
        </div>
        <div className="mt-0.5 text-xs leading-relaxed opacity-90 break-all">
          {error
            ? error.message
            : `已接入的客户端会连接 ${status.url}，但当前没有服务监听。`}
        </div>
      </div>
      <Button
        size="sm"
        variant="secondary"
        loading={restart.isPending}
        onClick={() => restart.mutate()}
      >
        <RefreshCw size={14} />
        重试
      </Button>
    </div>
  );
}
