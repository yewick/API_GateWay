import { useMemo } from "react";
import type { LogModeStats } from "../../types";
import { MODE_LABELS } from "../../lib/constants";
import { formatNumber } from "../../lib/constants";

// 手写条形图配色（遵循现有主题语义色，无图表库）
const MODE_BAR_COLORS: Record<string, string> = {
  chat: "bg-accent",
  messages: "bg-info",
  responses: "bg-success",
  embedding: "bg-warning",
};

interface ModeAgg {
  mode: string;
  requests: number;
  tokens: number;
}

/**
 * 多协议（mode）分布：按协议聚合请求量，渲染水平条形图。
 * 供仪表盘「请求协议分布」与用量页复用。
 */
export function ModeDistribution({ data }: { data: LogModeStats[] }) {
  const agg = useMemo<ModeAgg[]>(() => {
    const map = new Map<string, ModeAgg>();
    for (const row of data ?? []) {
      const cur = map.get(row.mode) ?? { mode: row.mode, requests: 0, tokens: 0 };
      cur.requests += row.requests;
      cur.tokens += row.tokens;
      map.set(row.mode, cur);
    }
    return Array.from(map.values()).sort((a, b) => b.requests - a.requests);
  }, [data]);

  const totalRequests = useMemo(
    () => agg.reduce((sum, m) => sum + m.requests, 0),
    [agg],
  );

  if (agg.length === 0) {
    return (
      <div className="flex items-center justify-center py-16 text-sm text-text-muted">
        暂无协议统计数据
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {agg.map((m) => {
        const pct = totalRequests > 0 ? (m.requests / totalRequests) * 100 : 0;
        const label = MODE_LABELS[m.mode]?.label ?? m.mode;
        const barColor = MODE_BAR_COLORS[m.mode] ?? "bg-accent";
        return (
          <div key={m.mode}>
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs text-text-secondary">{label}</span>
              <span className="text-xs text-text-muted tabular">
                {formatNumber(m.requests)} 请求 · {formatNumber(m.tokens)} tokens
              </span>
            </div>
            <div
              className="h-2 bg-bg-tertiary rounded-full overflow-hidden"
              title={`${label}：${m.requests} 请求，占 ${pct.toFixed(1)}%`}
            >
              <div
                className={`h-full rounded-full ${barColor}`}
                style={{ width: `${Math.max(2, pct)}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
