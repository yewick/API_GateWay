import { Activity, Zap, Server, Timer, type LucideIcon } from "lucide-react";
import { useDashboardStats } from "../../hooks/useDashboard";
import { Card } from "../ui/Card";
import { Spinner } from "../ui/Spinner";
import { formatNumber } from "../../lib/constants";

interface StatItem {
  key: string;
  label: string;
  icon: LucideIcon;
  accentClass: string;
}

const statItems: StatItem[] = [
  { key: "today_requests", label: "今日请求数", icon: Activity, accentClass: "text-accent bg-accent/10" },
  { key: "today_total_tokens", label: "今日 Token 消耗", icon: Zap, accentClass: "text-warning bg-warning/10" },
  { key: "active_channels", label: "活跃渠道数", icon: Server, accentClass: "text-success bg-success/10" },
  { key: "avg_latency_ms", label: "平均延迟", icon: Timer, accentClass: "text-info bg-info/10" },
];

const formatValue = (key: string, value: number): string => {
  if (key === "avg_latency_ms") return `${formatNumber(value)} ms`;
  if (key === "today_total_tokens") return formatNumber(value);
  return formatNumber(value);
};

export function StatsGrid() {
  const { data, isLoading, isError, error } = useDashboardStats();

  if (isLoading) {
    return (
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {[0, 1, 2, 3].map((i) => (
          <Card key={i} className="h-[110px] flex items-center justify-center">
            <Spinner />
          </Card>
        ))}
      </div>
    );
  }

  if (isError) {
    return (
      <Card className="p-6">
        <p className="text-sm text-danger">
          加载统计数据失败：{(error as Error)?.message ?? "未知错误"}
        </p>
      </Card>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      {statItems.map((item) => {
        const value = data?.[item.key as keyof typeof data] ?? 0;
        const Icon = item.icon;
        return (
          <Card key={item.key} className="group hover:-translate-y-0.5 hover:shadow-md transition-all">
            <div className="flex items-start justify-between">
              <div>
                <p className="text-xs text-text-secondary mb-2">{item.label}</p>
                <p className="text-2xl font-bold text-text-primary tabular">
                  {formatValue(item.key, value)}
                </p>
              </div>
              <div
                className={`w-10 h-10 rounded-lg flex items-center justify-center ${item.accentClass}`}
              >
                <Icon size={20} />
              </div>
            </div>
          </Card>
        );
      })}
    </div>
  );
}
