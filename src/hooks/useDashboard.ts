import { useQuery } from "@tanstack/react-query";
import { statsApi } from "../lib/api";

export const dashboardKeys = {
  stats: ["dashboard-stats"] as const,
};

export const useDashboardStats = () =>
  useQuery({
    queryKey: dashboardKeys.stats,
    queryFn: () => statsApi.getDashboard(),
    refetchInterval: 30_000, // 每 30 秒自动刷新
  });
