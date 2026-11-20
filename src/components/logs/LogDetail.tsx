import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ExternalLink, MessagesSquare, Wrench } from "lucide-react";
import type { RequestLog } from "../../types";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Modal } from "../ui/Modal";
import { Tabs } from "../ui/Tabs";
import { formatTime, statusColor } from "../../lib/constants";
import { logApi } from "../../lib/api";
import { useLog } from "../../hooks/useLogs";
import { normalizeLog } from "../../lib/logNormalize";
import { LogConversation } from "./LogConversation";
import { LogOverview } from "./LogOverview";
import { LogRawJson } from "./LogRawJson";
import { LogTimeline } from "./LogTimeline";
import { LogToolCalls } from "./LogToolCalls";

interface LogDetailProps {
  log: RequestLog | null;
  onClose: () => void;
  onOpenSession?: (sessionKey: string) => void;
}

export function LogDetail({ log, onClose, onOpenSession }: LogDetailProps) {
  const { data: fetched } = useLog(log?.id ?? null);
  const detail = fetched ?? log;

  const { data: findings } = useQuery({
    queryKey: ["log-findings", log?.id],
    queryFn: () => logApi.getFindings(log!.id),
    enabled: !!log?.id,
  });

  const [tab, setTab] = useState("overview");
  useEffect(() => {
    setTab("overview");
  }, [log?.id]);

  const normalized = useMemo(
    () =>
      detail
        ? normalizeLog(detail)
        : null,
    [detail?.id, detail?.request_body, detail?.response_body, detail?.response_choices],
  );

  if (!log || !detail || !normalized) return null;

  const visibleMessages = normalized.messages.filter((message) => !message.hidden);
  const requestTitle = detail.session_title || "未命名会话";

  const tabs = [
    { key: "overview", label: "概览" },
    { key: "conversation", label: "问答", badge: visibleMessages.length },
    { key: "tools", label: "工具调用", badge: normalized.toolCalls.length },
    { key: "timeline", label: "执行过程" },
    { key: "raw", label: "原始 JSON" },
  ];

  return (
    <Modal
      open
      onClose={onClose}
      title="请求日志详情"
      description={`ID: ${detail.id}`}
      size="xl"
    >
      <div className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={statusColor(detail.status_code) as never}>
            HTTP {detail.status_code}
          </Badge>
          <Badge variant="neutral">{detail.mode}</Badge>
          <Badge variant="outline">{detail.is_stream ? "流式" : "非流式"}</Badge>
          <span className="text-xs text-text-muted">
            {formatTime(detail.created_at)}
          </span>
          <span className="text-xs text-text-muted">
            {detail.model} · {detail.total_tokens.toLocaleString()} tokens · {detail.duration_ms}ms
          </span>
        </div>

        {detail.session_key && (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border-primary bg-bg-tertiary px-3 py-2">
            <div className="min-w-0">
              <div className="text-xs font-medium text-text-primary">
                所属会话：{requestTitle}
                {detail.session_client ? ` · ${detail.session_client}` : ""}
              </div>
              <div className="mt-0.5 text-[11px] text-text-muted mono break-all">
                {detail.session_key}
              </div>
            </div>
            {onOpenSession && (
              <Button
                size="sm"
                variant="secondary"
                onClick={() => onOpenSession(detail.session_key!)}
              >
                <ExternalLink size={14} />
                查看完整会话
              </Button>
            )}
          </div>
        )}

        {normalized.degraded && (
          <div className="rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            {normalized.degradedReason}
          </div>
        )}
        {normalized.warnings.length > 0 && (
          <div className="rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            {normalized.warnings.join("；")}
          </div>
        )}

        <Tabs tabs={tabs} activeKey={tab} onChange={setTab} />

        <div className="pt-1">
          {tab === "overview" && (
            <LogOverview log={detail} findings={findings} normalized={normalized} />
          )}
          {tab === "conversation" && (
            <LogConversation normalized={normalized} />
          )}
          {tab === "tools" && (
            <LogToolCalls toolCalls={normalized.toolCalls} />
          )}
          {tab === "timeline" && (
            <LogTimeline events={normalized.events} />
          )}
          {tab === "raw" && <LogRawJson log={detail} />}
        </div>

        <div className="flex items-center gap-3 border-t border-border-primary/60 pt-3 text-[11px] text-text-muted">
          <span className="flex items-center gap-1">
            <MessagesSquare size={12} />
            {visibleMessages.length} 条可见消息
          </span>
          <span className="flex items-center gap-1">
            <Wrench size={12} />
            {normalized.toolCalls.length} 个工具调用
          </span>
        </div>
      </div>
    </Modal>
  );
}
