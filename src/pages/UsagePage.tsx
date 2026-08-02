import { useMemo, useState } from "react";
import { Activity, Zap } from "lucide-react";
import { useLogStats } from "../hooks/useLogs";
import { PageHeader } from "../components/ui/PageHeader";
import { Card } from "../components/ui/Card";
import { Spinner } from "../components/ui/Spinner";
import { formatNumber } from "../lib/constants";

const DAY_RANGES = [
  { value: 7, label: "近 7 天" },
  { value: 14, label: "近 14 天" },
  { value: 30, label: "近 30 天" },
];

export const UsagePage = () => {
  const [days, setDays] = useState(7);
  const { data: stats, isLoading } = useLogStats(days);

  const totals = useMemo(() => {
    if (!stats) return { requests: 0, tokens: 0 };
    return stats.reduce(
      (acc, s) => ({
        requests: acc.requests + s.requests,
        tokens: acc.tokens + s.tokens,
      }),
      { requests: 0, tokens: 0 },
    );
  }, [stats]);

  const maxRequests = useMemo(
    () => Math.max(1, ...(stats ?? []).map((s) => s.requests)),
    [stats],
  );

  return (
    <div>
      <PageHeader
        title="用量分析"
        description="查看请求量与 Token 消耗趋势"
        actions={
          <div className="flex items-center gap-1 bg-bg-tertiary rounded-lg p-1">
            {DAY_RANGES.map((r) => (
              <button
                key={r.value}
                onClick={() => setDays(r.value)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                  days === r.value
                    ? "bg-bg-secondary text-accent shadow-sm"
                    : "text-text-secondary hover:text-text-primary"
                }`}
              >
                {r.label}
              </button>
            ))}
          </div>
        }
      />

      {/* 汇总卡片 */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
        <Card>
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg bg-accent/10 text-accent flex items-center justify-center">
              <Activity size={20} />
            </div>
            <div>
              <p className="text-xs text-text-secondary">请求总数</p>
              <p className="text-xl font-bold text-text-primary tabular">
                {isLoading ? "..." : formatNumber(totals.requests)}
              </p>
            </div>
          </div>
        </Card>
        <Card>
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg bg-warning/10 text-warning flex items-center justify-center">
              <Zap size={20} />
            </div>
            <div>
              <p className="text-xs text-text-secondary">Token 消耗总量</p>
              <p className="text-xl font-bold text-text-primary tabular">
                {isLoading ? "..." : formatNumber(totals.tokens)}
              </p>
            </div>
          </div>
        </Card>
      </div>

      {/* 柱状图 */}
      <Card title="每日请求量" noPadding>
        {isLoading ? (
          <div className="flex items-center justify-center py-24">
            <Spinner />
          </div>
        ) : stats && stats.length > 0 ? (
          <div className="p-5">
            <div className="flex items-end gap-1 h-48">
              {stats.map((s) => {
                const height = Math.max(4, (s.requests / maxRequests) * 100);
                return (
                  <div
                    key={s.date}
                    className="flex-1 flex flex-col items-center justify-end h-full group"
                    title={`${s.date}：${s.requests} 请求，${formatNumber(s.tokens)} tokens`}
                  >
                    <div className="w-full max-w-[28px] rounded-t bg-accent/70 group-hover:bg-accent transition-colors"
                      style={{ height: `${height}%` }}
                    >
                      <div className="w-full h-full bg-accent/70 group-hover:bg-accent rounded-t transition-colors opacity-0 group-hover:opacity-100" />
                    </div>
                    {days <= 14 && (
                      <span className="mt-1 text-[9px] text-text-muted tabular whitespace-nowrap">
                        {s.date.slice(5)}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
            {/* 图例提示 */}
            <p className="text-[11px] text-text-muted mt-3 text-center">
              {hasSomeRequests(stats)
                ? "悬停柱条查看该日期详细数据"
                : "当前时间范围内暂无请求数据"}
            </p>
          </div>
        ) : (
          <div className="flex items-center justify-center py-24 text-sm text-text-muted">
            暂无统计数据
          </div>
        )}
      </Card>

      {/* 每日明细表 */}
      <div className="mt-6">
        <Card title="每日明细" noPadding>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border-primary">
                  <th className="px-5 py-3 text-left text-xs font-medium text-text-muted">日期</th>
                  <th className="px-5 py-3 text-right text-xs font-medium text-text-muted">请求数</th>
                  <th className="px-5 py-3 text-right text-xs font-medium text-text-muted">Token 消耗</th>
                </tr>
              </thead>
              <tbody>
                {(stats ?? []).map((s) => (
                  <tr key={s.date} className="border-b border-border-primary/60 hover:bg-bg-hover/50 transition-colors">
                    <td className="px-5 py-2.5 text-xs text-text-primary tabular">{s.date}</td>
                    <td className="px-5 py-2.5 text-right text-xs text-text-secondary tabular">
                      {formatNumber(s.requests)}
                    </td>
                    <td className="px-5 py-2.5 text-right text-xs text-text-secondary tabular">
                      {formatNumber(s.tokens)}
                    </td>
                  </tr>
                ))}
                {(stats ?? []).length === 0 && (
                  <tr>
                    <td colSpan={3} className="px-5 py-10 text-center text-sm text-text-muted">
                      暂无统计数据
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </Card>
      </div>
    </div>
  );
};

const hasSomeRequests = (stats: { requests: number }[]): boolean =>
  stats.some((s) => s.requests > 0);
