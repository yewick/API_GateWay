import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { serverApi } from "../lib/api";

const isTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const serverStatusKeys = {
  all: ["server-status"] as const,
};

/// 网关运行状态：事件驱动刷新 + 15s 兜底轮询。
export const useServerStatus = () => {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen("server-status-changed", () => {
          queryClient.invalidateQueries({ queryKey: serverStatusKeys.all });
        }),
      )
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);

  return useQuery({
    queryKey: serverStatusKeys.all,
    queryFn: serverApi.status,
    refetchInterval: 15_000,
    retry: 0,
  });
};
