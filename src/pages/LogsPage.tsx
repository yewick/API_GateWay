import { useState } from "react";
import type { RequestLog } from "../types";
import { useLogs, type LogFilters } from "../hooks/useLogs";
import { useThrottledRefresh } from "../hooks/useThrottledRefresh";
import { PageHeader } from "../components/ui/PageHeader";
import { LogFiltersBar } from "../components/logs/LogFilters";
import { LogTable } from "../components/logs/LogTable";
import { LogDetail } from "../components/logs/LogDetail";
import { Pagination } from "../components/ui/Pagination";
import { Button } from "../components/ui/Button";
import { exportApi } from "../lib/api";
import { save } from "@tauri-apps/plugin-dialog";
import { RefreshCw, Download } from "lucide-react";
import { toast } from "../lib/toast";

const CSV_HEADER = [
  "id", "seq", "created_at", "api_key_name", "channel_name", "model",
  "upstream_model", "mode", "trace_id", "status_code", "prompt_tokens",
  "completion_tokens", "total_tokens", "duration_ms", "is_stream", "is_retry",
  "risk_level", "risk_score", "security_action", "error_message",
];

const buildCsv = (rows: RequestLog[]): string => {
  const esc = (v: unknown) => {
    const s = v == null ? "" : String(v);
    return `"${s.replace(/"/g, '""')}"`;
  };
  const lines = [
    CSV_HEADER.join(","),
    ...rows.map((r) =>
      CSV_HEADER.map((k) => esc((r as unknown as Record<string, unknown>)[k])).join(","),
    ),
  ];
  return "﻿" + lines.join("\n");
};

export const LogsPage = () => {
  const [filters, setFilters] = useState<LogFilters>({ page: 1, page_size: 20 });
  const [selectedLog, setSelectedLog] = useState<RequestLog | null>(null);
  const [exporting, setExporting] = useState<null | "csv" | "json">(null);
  const { refresh, refreshing } = useThrottledRefresh([["logs"]]);

  const { data: logs, isLoading } = useLogs(filters);

  // 分页总数估算：当前页返回数量达到页大小，则视为还有更多
  const pageSize = filters.page_size ?? 20;
  const page = filters.page ?? 1;
  const hasMore = (logs?.length ?? 0) >= pageSize;
  const estimatedTotal =
    !hasMore && page === 1 ? (logs?.length ?? 0) : page * pageSize + (hasMore ? pageSize : 0);

  // 导出当前已加载日志（受分页限制，仅导出当前页；后续可扩展为全量导出）
  const doExport = async (kind: "csv" | "json") => {
    const rows = logs ?? [];
    if (rows.length === 0) {
      toast.warning("无可导出数据", "当前筛选条件下没有日志记录");
      return;
    }
    setExporting(kind);
    try {
      // 原生「另存为」对话框：跨平台一致，由用户自选保存路径
      const path = await save({
        defaultPath: `yeapi-logs-${Date.now()}.${kind}`,
        filters: [{ name: kind.toUpperCase(), extensions: [kind] }],
      });
      if (!path) return; // 用户取消：静默返回
      const content = kind === "csv" ? buildCsv(rows) : JSON.stringify(rows, null, 2);
      await exportApi.writeTextFile(path, content);
      toast.success("导出成功", `已保存到 ${path}`);
    } catch (err) {
      toast.error("导出失败", (err as Error)?.message);
    } finally {
      setExporting(null);
    }
  };

  return (
    <div>
      <PageHeader
        title="请求日志"
        description="查看所有经过网关的请求记录"
        actions={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => doExport("csv")}
              loading={exporting === "csv"}
              title="导出 CSV（当前页）"
            >
              <Download size={14} />
              CSV
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => doExport("json")}
              loading={exporting === "json"}
              title="导出 JSON（当前页）"
            >
              <Download size={14} />
              JSON
            </Button>
            <button
              onClick={refresh}
              className={`p-2 rounded-lg text-text-secondary hover:text-text-primary hover:bg-bg-hover transition-colors ${
                refreshing ? "animate-spin" : ""
              }`}
              title="刷新日志"
              aria-label="刷新日志"
            >
              <RefreshCw size={16} />
            </button>
          </>
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
