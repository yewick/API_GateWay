import { useState } from "react";
import type { RequestLog } from "../types";
import {
  useLogs,
  useLogSessions,
  type LogFilters,
  type SessionFilters,
} from "../hooks/useLogs";
import { useThrottledRefresh } from "../hooks/useThrottledRefresh";
import { PageHeader } from "../components/ui/PageHeader";
import { Tabs } from "../components/ui/Tabs";
import { LogFiltersBar } from "../components/logs/LogFilters";
import { LogTable } from "../components/logs/LogTable";
import { LogDetail } from "../components/logs/LogDetail";
import { SessionList } from "../components/logs/SessionList";
import { SessionReader } from "../components/logs/SessionReader";
import { Pagination } from "../components/ui/Pagination";
import { Button } from "../components/ui/Button";
import { exportApi, logApi } from "../lib/api";
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
  const [view, setView] = useState<"requests" | "sessions">("requests");
  const [filters, setFilters] = useState<LogFilters>({ page: 1, page_size: 20 });
  const [sessionFilters, setSessionFilters] = useState<SessionFilters>({
    page: 1,
    page_size: 20,
  });
  const [selectedLog, setSelectedLog] = useState<RequestLog | null>(null);
  const [selectedSessionKey, setSelectedSessionKey] = useState<string | null>(
    null,
  );
  const [exporting, setExporting] = useState<null | "csv" | "json">(null);
  const { refresh, refreshing } = useThrottledRefresh([
    ["logs"],
    ["log-sessions"],
    ["log-session"],
  ]);

  const { data: logs, isLoading } = useLogs(filters);
  const { data: sessions = [], isLoading: sessionsLoading } =
    useLogSessions(sessionFilters);

  const pageSize = filters.page_size ?? 20;
  const page = filters.page ?? 1;
  const hasMore = (logs?.length ?? 0) >= pageSize;
  const estimatedTotal =
    !hasMore && page === 1 ? (logs?.length ?? 0) : page * pageSize + (hasMore ? pageSize : 0);

  const sessionPageSize = sessionFilters.page_size ?? 20;
  const sessionPage = sessionFilters.page ?? 1;
  const sessionsHasMore = sessions.length >= sessionPageSize;
  const sessionsEstimatedTotal =
    !sessionsHasMore && sessionPage === 1
      ? sessions.length
      : sessionPage * sessionPageSize + (sessionsHasMore ? sessionPageSize : 0);

  const doExport = async (kind: "csv" | "json") => {
    const rows = logs ?? [];
    if (rows.length === 0) {
      toast.warning("无可导出数据", "当前筛选条件下没有日志记录");
      return;
    }
    setExporting(kind);
    try {
      const path = await save({
        defaultPath: `yeapi-logs-${Date.now()}.${kind}`,
        filters: [{ name: kind.toUpperCase(), extensions: [kind] }],
      });
      if (!path) return;
      const content = kind === "csv" ? buildCsv(rows) : JSON.stringify(rows, null, 2);
      await exportApi.writeTextFile(path, content);
      toast.success("导出成功", `已保存到 ${path}`);
    } catch (err) {
      toast.error("导出失败", (err as Error)?.message);
    } finally {
      setExporting(null);
    }
  };

  const openRequest = async (id: string) => {
    try {
      setSelectedLog(await logApi.get(id));
    } catch (error) {
      toast.error("打开请求失败", (error as Error).message);
    }
  };

  const openSession = (sessionKey: string) => {
    setSelectedLog(null);
    setView("sessions");
    setSelectedSessionKey(sessionKey);
  };

  return (
    <div>
      <PageHeader
        title="请求日志"
        description="查看请求记录，并按会话阅读多轮问答与工具执行过程"
        actions={
          <>
            {view === "requests" && (
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
              </>
            )}
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

      <Tabs
        tabs={[
          { key: "requests", label: "请求日志" },
          { key: "sessions", label: "会话" },
        ]}
        activeKey={view}
        onChange={(key) => setView(key as "requests" | "sessions")}
        className="mb-4"
      />

      {view === "requests" ? (
        <>
          <LogFiltersBar value={filters} onChange={setFilters} />
          <LogTable
            data={logs ?? []}
            loading={isLoading}
            onRowClick={(log) => setSelectedLog(log)}
          />
          <div className="mt-3 bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
            <Pagination
              page={page}
              pageSize={pageSize}
              total={estimatedTotal}
              onChange={(next) => setFilters((current) => ({ ...current, page: next }))}
            />
          </div>
        </>
      ) : selectedSessionKey ? (
        <SessionReader
          sessionKey={selectedSessionKey}
          onBack={() => setSelectedSessionKey(null)}
          onOpenRequest={openRequest}
        />
      ) : (
        <>
          <SessionList
            filters={sessionFilters}
            onChange={setSessionFilters}
            sessions={sessions}
            loading={sessionsLoading}
            onOpen={setSelectedSessionKey}
          />
          <div className="mt-3 bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
            <Pagination
              page={sessionPage}
              pageSize={sessionPageSize}
              total={sessionsEstimatedTotal}
              onChange={(next) =>
                setSessionFilters((current) => ({ ...current, page: next }))
              }
            />
          </div>
        </>
      )}

      <LogDetail
        log={selectedLog}
        onClose={() => setSelectedLog(null)}
        onOpenSession={openSession}
      />
    </div>
  );
};
