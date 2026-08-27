import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "../lib/toast";

type QueryKey = readonly unknown[];

/**
 * 手动刷新 + 节流（throttle）。
 * 首次点击立即失效对应查询；`ms` 内重复点击只算一次，并给出轻提示。
 * React Query 前缀匹配：传入 `["logs"]` 即可覆盖 `["logs", filters]`、`["logs", id]` 等全部子键。
 */
export function useThrottledRefresh(keys: QueryKey[], ms = 10_000) {
  const qc = useQueryClient();
  const lastRef = useRef(0);
  const [refreshing, setRefreshing] = useState(false);

  const refresh = async () => {
    const now = Date.now();
    if (now - lastRef.current < ms) {
      toast.warning("操作过于频繁", "请稍后再试");
      return;
    }
    lastRef.current = now;
    setRefreshing(true);
    try {
      await Promise.all(
        keys.map((key) => qc.invalidateQueries({ queryKey: [...key] })),
      );
      // 保持最短的旋转反馈，避免一瞬而过
      await new Promise((r) => setTimeout(r, 400));
    } finally {
      setRefreshing(false);
    }
  };

  return { refresh, refreshing };
}
