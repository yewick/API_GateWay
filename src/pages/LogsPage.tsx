import { useState } from "react";
import type { RequestLog } from "../types";
import { useLogs, type LogFilters } from "../hooks/useLogs";
import { PageHeader } from "../components/ui/PageHeader";
import { LogFiltersBar } from "../components/logs/LogFilters";
import { LogTable } from "../components/logs/LogTable";
import { LogDetail } from "../components/logs/LogDetail";
import { Pagination } from "../components/ui/Pagination";
import { useQueryClient } from "@tanstack/react-query";
import { logKeys } from "../hooks/useLogs";
import { RefreshCw } from "lucide-react";

export const LogsPage = () => {
  const qc = useQueryClient();
  const [filters, setFilters] = useState<LogFilters>({ page: 1, page_size: 20 });
  const [selectedLog, setSelectedLog] = useState<RequestLog | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const { data: logs, isLoading } = useLogs(filters);

  // 分页总数估算：当前页返回数量达到页大小，则视为还有更多
  const pageSize = filters.page_size ?? 20;
  const page = filters.page ?? 1;
  const hasMore = (logs?.length ?? 0) >= pageSize;
  const estimatedTotal =
    !hasMore && page === 1 ? (logs?.length ?? 0) : page * pageSize + (hasMore ? pageSize : 0);

  const handleRefresh = async () => {
    setRefreshing(true);
    await qc.invalidateQueries({ queryKey: logKeys.all });
    setTimeout(() => setRefreshing(false), 400);
  };

  return (
    <div>
      <PageHeader
        title="请求日志"
        description="查看所有经过网关的请求记录"
        actions={
          <button
            onClick={handleRefresh}
            className={`p-2 rounded-lg text-text-secondary hover:text-text-primary hover:bg-bg-hover transition-colors ${
              refreshing ? "animate-spin" : ""
            }`}
            title="刷新日志"
            aria-label="刷新日志"
          >
            <RefreshCw size={16} />
          </button>
        }
      />

      {/* 筛选栏 */}
      <LogFiltersBar value={filters} onChange={setFilters} />

      {/* 日志表格 */}
      <LogTable
        data={logs ?? []}
        loading={isLoading}
        onRowClick={(log) => setSelectedLog(log)}
      />

      {/* 分页 */}
      <div className="mt-3 bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
        <Pagination
          page={page}
          pageSize={pageSize}
          total={estimatedTotal}
          onChange={(page) => setFilters((f) => ({ ...f, page }))}
        />
      </div>

      {/* 详情弹窗 */}
      <LogDetail log={selectedLog} onClose={() => setSelectedLog(null)} />
    </div>
  );
};
