import { useQuery } from "@tanstack/react-query";
import { logApi } from "../lib/api";

export interface LogFilters {
  keyword?: string;
  channel_name?: string;
  model?: string;
  mode?: string;
  status_code?: number;
  risk_level?: string;
  security_action?: string;
  finding_rule?: string;
  start_date?: string;
  end_date?: string;
  page?: number;
  page_size?: number;
}

export const logKeys = {
  all: ["logs"] as const,
  filtered: (filters: LogFilters) => ["logs", filters] as const,
  detail: (id: string) => ["logs", id] as const,
  stats: (days?: number) => ["log-stats", days ?? 30] as const,
  modeStats: (days?: number) => ["log-mode-stats", days ?? 30] as const,
};

export const useLogs = (filters: LogFilters) =>
  useQuery({
    queryKey: logKeys.filtered(filters),
    queryFn: () => logApi.getAll(filters),
    placeholderData: (prev) => prev, // 切换筛选条件时保留旧数据，避免闪烁
    refetchInterval: 30_000, // 每 30 秒自动刷新
  });

export const useLog = (id: string | null) =>
  useQuery({
    queryKey: logKeys.detail(id ?? ""),
    queryFn: () => logApi.get(id ?? ""),
    enabled: !!id,
  });

export const useLogStats = (days = 30) =>
  useQuery({
    queryKey: logKeys.stats(days),
    queryFn: () => logApi.getStats(days),
    refetchInterval: 30_000, // 每 30 秒自动刷新
  });

export const useModeStats = (days = 30) =>
  useQuery({
    queryKey: logKeys.modeStats(days),
    queryFn: () => logApi.getModeStats(days),
    refetchInterval: 30_000, // 每 30 秒自动刷新
  });
