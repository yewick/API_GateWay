import type { RequestLog } from "../../types";
import { formatTime, statusColor, MODE_LABELS } from "../../lib/constants";
import { Table, type Column } from "../ui/Table";
import { Badge } from "../ui/Badge";

interface LogTableProps {
  data: RequestLog[];
  loading?: boolean;
  onRowClick: (log: RequestLog) => void;
}

const formatDuration = (ms: number): string => {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)}s`;
  return `${ms}ms`;
};

export function LogTable({ data, loading, onRowClick }: LogTableProps) {
  const columns: Column<RequestLog>[] = [
    {
      key: "created_at",
      title: "时间",
      width: "150px",
      render: (v) => (
        <span className="text-xs text-text-secondary tabular">{formatTime(String(v))}</span>
      ),
    },
    {
      key: "api_key_name",
      title: "密钥",
      render: (v) => (
        <span className="text-xs text-text-primary">{String(v ?? "-")}</span>
      ),
    },
    {
      key: "channel_name",
      title: "渠道",
      render: (v) => (
        <span className="text-xs text-text-primary">{String(v ?? "-")}</span>
      ),
    },
    {
      key: "model",
      title: "模型",
      render: (v) => (
        <code className="text-xs mono text-text-secondary">{String(v)}</code>
      ),
    },
    {
      key: "mode",
      title: "协议",
      width: "100px",
      render: (v) => {
        const mode = String(v ?? "chat");
        const info = MODE_LABELS[mode] ?? { label: mode, color: "neutral" };
        return <Badge variant={info.color as never}>{info.label}</Badge>;
      },
    },
    {
      key: "trace_id",
      title: "Trace ID",
      width: "140px",
      render: (v) => (
        <code className="text-xs mono text-text-muted truncate block max-w-[130px]" title={String(v ?? "-")}>
          {v ? String(v) : "-"}
        </code>
      ),
    },
    {
      key: "status_code",
      title: "状态",
      width: "90px",
      render: (v) => {
        const code = Number(v);
        return <Badge variant={statusColor(code) as never}>{code}</Badge>;
      },
    },
    {
      key: "total_tokens",
      title: "Tokens",
      width: "100px",
      align: "right",
      render: (v) => (
        <span className="text-xs tabular text-text-secondary">{Number(v).toLocaleString()}</span>
      ),
    },
    {
      key: "duration_ms",
      title: "延迟",
      width: "90px",
      align: "right",
      render: (v) => (
        <span className="text-xs tabular text-text-secondary">
          {formatDuration(Number(v))}
        </span>
      ),
    },
    {
      key: "risk_level",
      title: "风险",
      width: "90px",
      render: (v) => {
        const level = String(v ?? "none");
        if (level === "none") return <span className="text-xs text-text-muted">-</span>;
        const variant =
          level === "critical" || level === "high"
            ? "danger"
            : level === "medium"
              ? "warning"
              : "success";
        return <Badge variant={variant as never}>{level}</Badge>;
      },
    },
  ];

  return (
    <div className="bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
      <Table
        columns={columns}
        data={data}
        rowKey={(r) => r.id}
        onRowClick={onRowClick}
        loading={loading}
        emptyText="暂无日志记录"
        emptyDescription="调整筛选条件，或等待新的请求产生日志"
      />
    </div>
  );
}
