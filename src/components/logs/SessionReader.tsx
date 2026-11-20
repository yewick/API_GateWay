import { useMemo, useState } from "react";
import {
  ArrowLeft,
  Bot,
  Check,
  ChevronDown,
  Copy,
  Eye,
  EyeOff,
  MessagesSquare,
  Wrench,
} from "lucide-react";
import { useLogSession } from "../../hooks/useLogs";
import { normalizeSession, type SessionTurn } from "../../lib/logNormalize";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { MarkdownContent } from "../ui/Markdown";
import { Spinner } from "../ui/Spinner";
import { formatTime, statusColor } from "../../lib/constants";
import { toast } from "../../lib/toast";
import { LogToolCalls } from "./LogToolCalls";

interface SessionReaderProps {
  sessionKey: string;
  onBack: () => void;
  onOpenRequest: (id: string) => void;
}

function SessionTurnView({
  turn,
  showHidden,
  requestMeta,
  onOpenRequest,
}: {
  turn: SessionTurn;
  showHidden: boolean;
  requestMeta: ReturnType<typeof normalizeSession>["requestMeta"];
  onOpenRequest: (id: string) => void;
}) {
  const messages = turn.messages.filter(
    (message) => showHidden || !message.hidden,
  );
  const userMessages = messages.filter((message) => message.role === "user");
  const assistantMessages = messages.filter(
    (message) => message.role === "assistant" && (message.text || message.thinking),
  );
  const hiddenMessages = messages.filter(
    (message) => message.hidden && message.role !== "tool",
  );
  const failureCount = turn.toolCalls.filter(
    (tool) => tool.status === "error",
  ).length;

  return (
    <section id={`session-turn-${turn.index}`} className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium text-text-muted">
          第 {turn.index} 轮
        </span>
        <span className="h-px flex-1 bg-border-primary/60" />
      </div>

      {hiddenMessages.map((message) => (
        <div
          key={message.id}
          className="rounded-lg border border-dashed border-border-primary/60 bg-bg-primary/30 px-3 py-2"
        >
          <div className="text-[11px] text-text-muted">
            {message.role} · {message.hiddenReason ?? "注入内容"}
          </div>
          <pre className="mt-1 max-h-32 overflow-auto text-xs text-text-muted whitespace-pre-wrap break-all">
            {message.text}
          </pre>
        </div>
      ))}

      {userMessages.map((message) => (
        <div key={message.id} className="flex justify-end">
          <div className="max-w-[82%] rounded-xl bg-accent px-4 py-3 text-sm leading-relaxed text-white whitespace-pre-wrap">
            {message.text}
          </div>
        </div>
      ))}

      {turn.toolCalls.length > 0 && (
        <details className="rounded-xl border border-border-primary bg-bg-secondary">
          <summary className="flex cursor-pointer items-center gap-2 px-4 py-2.5 text-xs text-text-secondary">
            <Wrench size={14} className="text-text-muted" />
            执行过程 · {turn.toolCalls.length} 步 · {turn.toolCalls.length} 个工具
            {failureCount > 0 && (
              <span className="text-danger">· {failureCount} 个失败</span>
            )}
            <ChevronDown size={14} className="ml-auto text-text-muted" />
          </summary>
          <div className="border-t border-border-primary/60 p-3">
            <LogToolCalls toolCalls={turn.toolCalls} />
          </div>
        </details>
      )}

      {assistantMessages.map((message) => {
        const meta = message.requestId
          ? requestMeta[message.requestId]
          : undefined;
        return (
          <div key={message.id} className="flex gap-3">
            <div className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent/15">
              <Bot size={16} className="text-accent" />
            </div>
            <div className="min-w-0 max-w-[86%] rounded-xl border border-border-primary bg-bg-tertiary px-4 py-3">
              {message.text &&
                (message.text.length > 60_000 ? (
                  <pre className="text-xs text-text-primary whitespace-pre-wrap break-all">
                    {message.text}
                  </pre>
                ) : (
                  <div className="text-sm text-text-primary">
                    <MarkdownContent>{message.text}</MarkdownContent>
                  </div>
                ))}
              {message.thinking && (
                <details className="mt-2 rounded-lg border border-border-primary/60 bg-bg-primary/40 px-3 py-2">
                  <summary className="cursor-pointer text-xs text-text-muted">
                    思考过程（{message.thinking.length.toLocaleString()} 字符）
                  </summary>
                  <pre className="mt-2 text-xs text-text-secondary whitespace-pre-wrap break-all">
                    {message.thinking}
                  </pre>
                </details>
              )}
              {meta && (
                <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-text-muted">
                  <span>
                    {meta.model} · {meta.totalTokens.toLocaleString()} tokens ·{" "}
                    {meta.durationMs}ms
                  </span>
                  <Badge variant={statusColor(meta.statusCode) as never}>
                    {meta.statusCode}
                  </Badge>
                  <button
                    type="button"
                    onClick={() => onOpenRequest(message.requestId!)}
                    className="text-accent hover:underline"
                  >
                    查看请求
                  </button>
                </div>
              )}
            </div>
          </div>
        );
      })}
    </section>
  );
}

export function SessionReader({
  sessionKey,
  onBack,
  onOpenRequest,
}: SessionReaderProps) {
  const { data, isLoading, error } = useLogSession(sessionKey);
  const [showHidden, setShowHidden] = useState(false);
  const [copied, setCopied] = useState(false);

  const normalized = useMemo(
    () => (data ? normalizeSession(data) : null),
    [data],
  );

  if (isLoading) {
    return (
      <div className="flex justify-center py-14">
        <Spinner />
      </div>
    );
  }
  if (error || !data || !normalized) {
    return (
      <div className="space-y-3">
        <Button size="sm" variant="ghost" onClick={onBack}>
          <ArrowLeft size={14} />
          返回会话列表
        </Button>
        <EmptyState
          icon={MessagesSquare}
          title="会话加载失败"
          description={(error as Error)?.message ?? "会话不存在"}
        />
      </div>
    );
  }

  const copyId = async () => {
    try {
      await navigator.clipboard.writeText(data.summary.session_key);
      setCopied(true);
      toast.success("已复制", "会话 ID 已复制");
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("复制失败", "请手动选择并复制");
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-border-primary bg-bg-secondary p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Button size="sm" variant="ghost" onClick={onBack}>
                <ArrowLeft size={14} />
                返回
              </Button>
              <h2 className="truncate text-base font-semibold text-text-primary">
                {data.summary.session_title || "未命名会话"}
              </h2>
              <Badge variant="outline">
                {data.summary.session_client || "未知客户端"}
              </Badge>
              {data.summary.last_status_code != null && (
                <Badge
                  variant={statusColor(data.summary.last_status_code) as never}
                >
                  {data.summary.last_status_code}
                </Badge>
              )}
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-muted">
              <span>{formatTime(data.summary.first_at)}</span>
              <span>→ {formatTime(data.summary.last_at)}</span>
              <span>{data.summary.request_count} 个请求</span>
              <span>{data.summary.turn_count} 轮</span>
              <span>{data.summary.total_tokens.toLocaleString()} tokens</span>
              {data.summary.models.map((model) => (
                <Badge key={model} variant="neutral">
                  {model}
                </Badge>
              ))}
            </div>
            <button
              type="button"
              onClick={copyId}
              className="mt-1 flex items-center gap-1 text-[11px] text-text-muted mono hover:text-text-primary"
              title="复制会话 ID"
            >
              {copied ? <Check size={11} /> : <Copy size={11} />}
              {data.summary.session_key}
            </button>
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowHidden((value) => !value)}
            >
              {showHidden ? <EyeOff size={14} /> : <Eye size={14} />}
              {showHidden ? "隐藏系统消息" : "显示系统消息"}
            </Button>
          </div>
        </div>
        {normalized.degraded && (
          <div className="mt-3 rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            {normalized.degradedReason}
          </div>
        )}
      </div>

      <div className="flex gap-4">
        <div className="min-w-0 flex-1 space-y-6">
          {normalized.turns.length === 0 ? (
            <EmptyState
              icon={MessagesSquare}
              title="没有可还原的对话"
              description="该会话缺少请求快照或用户消息"
            />
          ) : (
            normalized.turns.map((turn) => (
              <SessionTurnView
                key={turn.index}
                turn={turn}
                showHidden={showHidden}
                requestMeta={normalized.requestMeta}
                onOpenRequest={onOpenRequest}
              />
            ))
          )}

          {normalized.auxiliary.length > 0 && (
            <details className="rounded-xl border border-border-primary bg-bg-secondary">
              <summary className="cursor-pointer px-4 py-2.5 text-xs text-text-secondary">
                辅助请求（{normalized.auxiliary.length}）：标题生成、建议、CLAUDE.md 等
              </summary>
              <div className="space-y-1 border-t border-border-primary/60 p-3">
                {normalized.auxiliary.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex flex-wrap items-center gap-2 rounded-lg bg-bg-tertiary px-3 py-2 text-xs"
                  >
                    <span className="text-text-muted">
                      {formatTime(entry.created_at)}
                    </span>
                    <span className="mono text-text-secondary">
                      {entry.model}
                    </span>
                    <Badge variant={statusColor(entry.status_code) as never}>
                      {entry.status_code}
                    </Badge>
                    <Badge variant="outline">{entry.request_kind}</Badge>
                    <button
                      type="button"
                      onClick={() => onOpenRequest(entry.id)}
                      className="ml-auto text-accent hover:underline"
                    >
                      查看请求
                    </button>
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>

        {normalized.turns.length > 2 && (
          <aside className="hidden w-56 shrink-0 xl:block">
            <div className="sticky top-4 rounded-xl border border-border-primary bg-bg-secondary p-3">
              <div className="mb-2 text-xs font-medium text-text-secondary">
                对话目录
              </div>
              <div className="space-y-1">
                {normalized.turns.map((turn) => (
                  <button
                    key={turn.index}
                    type="button"
                    onClick={() =>
                      document
                        .getElementById(`session-turn-${turn.index}`)
                        ?.scrollIntoView({ behavior: "smooth", block: "start" })
                    }
                    className="block w-full truncate rounded-md px-2 py-1 text-left text-[11px] text-text-muted hover:bg-bg-hover hover:text-text-primary"
                    title={turn.title}
                  >
                    {turn.index}. {turn.title}
                  </button>
                ))}
              </div>
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
