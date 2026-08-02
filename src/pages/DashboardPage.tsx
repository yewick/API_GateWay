import { useNavigate } from "react-router-dom";
import { ArrowRight } from "lucide-react";
import { StatsGrid } from "../components/dashboard/StatsGrid";
import { PageHeader } from "../components/ui/PageHeader";
import { Card } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { LogTable } from "../components/logs/LogTable";
import { useLogs, type LogFilters } from "../hooks/useLogs";
import { useDashboardStats } from "../hooks/useDashboard";
import { useState } from "react";
import type { RequestLog } from "../types";
import { LogDetail } from "../components/logs/LogDetail";

export const DashboardPage = () => {
  const navigate = useNavigate();
  const [selectedLog, setSelectedLog] = useState<RequestLog | null>(null);
  const { data: stats } = useDashboardStats();

  // 最近 5 条日志
  const recentFilters: LogFilters = { page: 1, page_size: 5 };
  const { data: recentLogs, isLoading: logsLoading } = useLogs(recentFilters);

  return (
    <div>
      <PageHeader
        title="仪表盘"
        description="网关运行状态总览"
      />

      {/* 统计卡片 */}
      <StatsGrid />

      {/* 补充统计 */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mt-4">
        <Card title="渠道总数">
          <p className="text-2xl font-bold text-text-primary tabular">
            {stats?.total_channels ?? 0}
          </p>
          <p className="text-xs text-text-muted mt-1">全部已注册渠道</p>
        </Card>
        <Card title="密钥总数">
          <p className="text-2xl font-bold text-text-primary tabular">
            {stats?.total_api_keys ?? 0}
          </p>
          <p className="text-xs text-text-muted mt-1">已发放的网关密钥</p>
        </Card>
        <Card title="累计请求">
          <p className="text-2xl font-bold text-text-primary tabular">
            {(stats?.total_requests ?? 0).toLocaleString()}
          </p>
          <p className="text-xs text-text-muted mt-1">网关处理总请求数</p>
        </Card>
      </div>

      {/* 最近请求 */}
      <div className="mt-6">
        <Card
          title="最近请求"
          noPadding
          headerRight={
            <Button
              variant="ghost"
              size="sm"
              onClick={() => navigate("/logs")}
            >
              查看全部
              <ArrowRight size={14} />
            </Button>
          }
        >
          <LogTable
            data={recentLogs ?? []}
            loading={logsLoading}
            onRowClick={(log) => setSelectedLog(log)}
          />
        </Card>
      </div>

      {/* 日志详情弹窗 */}
      <LogDetail log={selectedLog} onClose={() => setSelectedLog(null)} />
    </div>
  );
};
