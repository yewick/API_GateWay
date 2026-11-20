import {
  Bot,
  MessagesSquare,
  Search,
  Terminal,
} from "lucide-react";
import type { ReactNode } from "react";
import type { LogSessionSummary } from "../../types";
import type { SessionFilters } from "../../hooks/useLogs";
import { Badge } from "../ui/Badge";
import { EmptyState } from "../ui/EmptyState";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Spinner } from "../ui/Spinner";
import { formatTime, statusColor } from "../../lib/constants";

interface SessionListProps {
  filters: SessionFilters;
  onChange: (filters: SessionFilters) => void;
  sessions: LogSessionSummary[];
  loading: boolean;
  onOpen: (sessionKey: string) => void;
}

const clientMeta: Record<string, { label: string; icon: ReactNode }> = {
  codex: { label: "Codex", icon: <Terminal size={16} /> },
  "claude-code": { label: "Claude Code", icon: <Bot size={16} /> },
  chat: { label: "Chat", icon: <MessagesSquare size={16} /> },
};

const clientInfo = (client: string | null) =>
  (client && clientMeta[client]) || {
    label: client || "未知客户端",
    icon: <MessagesSquare size={16} />,
  };

export function SessionList({
  filters,
  onChange,
  sessions,
  loading,
  onOpen,
}: SessionListProps) {
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end gap-3 rounded-xl border border-border-primary bg-bg-secondary p-4">
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted"
          />
          <Input
            value={filters.keyword ?? ""}
            onChange={(event) =>
              onChange({ ...filters, keyword: event.target.value, page: 1 })
            }
            placeholder="搜索会话标题或 ID…"
            className="w-64 pl-8"
          />
        </div>
        <div className="w-44">
          <Select
            value={filters.client ?? ""}
            onChange={(event) =>
              onChange({
                ...filters,
                client: event.target.value || undefined,
                page: 1,
              })
            }
            options={[
              { value: "", label: "全部客户端" },
              { value: "codex", label: "Codex" },
              { value: "claude-code", label: "Claude Code" },
              { value: "chat", label: "Chat" },
            ]}
          />
        </div>
        <div>
          <div className="mb-1.5 text-xs text-text-muted">开始日期</div>
          <Input
            type="date"
            value={filters.start_date ?? ""}
            onChange={(event) =>
              onChange({
                ...filters,
                start_date: event.target.value || undefined,
                page: 1,
              })
            }
          />
        </div>
        <div>
          <div className="mb-1.5 text-xs text-text-muted">结束日期</div>
          <Input
            type="date"
            value={filters.end_date ?? ""}
            onChange={(event) =>
              onChange({
                ...filters,
                end_date: event.target.value || undefined,
                page: 1,
              })
            }
          />
        </div>
      </div>

      {loading && sessions.length === 0 ? (
        <div className="flex justify-center py-14">
          <Spinner />
        </div>
      ) : sessions.length === 0 ? (
        <div className="rounded-xl border border-border-primary bg-bg-secondary">
          <EmptyState
            icon={MessagesSquare}
            title="还没有可展示的会话"
            description="新版本记录会自动归并同一会话；旧日志会在启动后分批补算"
          />
        </div>
      ) : (
        <div className="space-y-2">
          {sessions.map((session) => {
            const client = clientInfo(session.session_client);
            return (
              <button
                key={session.session_key}
                type="button"
                onClick={() => onOpen(session.session_key)}
                className="w-full rounded-xl border border-border-primary bg-bg-secondary px-4 py-3 text-left transition-colors hover:bg-bg-hover"
              >
                <div className="flex items-start gap-3">
                  <span className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-bg-tertiary text-text-secondary">
                    {client.icon}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="truncate text-sm font-semibold text-text-primary">
                        {session.session_title || "未命名会话"}
                      </span>
                      <Badge variant="outline">{client.label}</Badge>
                      {session.models.slice(0, 3).map((model) => (
                        <Badge key={model} variant="neutral">
                          {model}
                        </Badge>
                      ))}
                    </div>
                    <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-text-muted">
                      <span>最后活跃 {formatTime(session.last_at)}</span>
                      <span>{session.request_count} 个请求</span>
                      <span>{session.turn_count} 轮</span>
                      <span>{session.total_tokens.toLocaleString()} tokens</span>
                    </div>
                    <div className="mt-1 text-[11px] text-text-muted mono truncate">
                      {session.session_key}
                    </div>
                  </div>
                  <div className="shrink-0">
                    <Badge
                      variant={
                        session.last_status_code == null
                          ? "neutral"
                          : (statusColor(session.last_status_code) as never)
                      }
                    >
                      {session.last_status_code ?? "-"}
                    </Badge>
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
