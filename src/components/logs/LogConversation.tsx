import { useState } from "react";
import { Bot, Eye, EyeOff, Wrench } from "lucide-react";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { MarkdownContent } from "../ui/Markdown";
import type { NormalizedLog, ViewMessage } from "../../lib/logNormalize";

interface LogConversationProps {
  normalized: NormalizedLog;
}

function ThinkingBlock({ text }: { text: string }) {
  if (!text) return null;
  return (
    <details className="mt-2 rounded-lg border border-border-primary/60 bg-bg-primary/40 px-3 py-2">
      <summary className="cursor-pointer text-xs text-text-muted">
        思考过程（{text.length.toLocaleString()} 字符）
      </summary>
      <pre className="mt-2 text-xs text-text-secondary whitespace-pre-wrap break-all">
        {text}
      </pre>
    </details>
  );
}

function ToolRow({ message }: { message: ViewMessage }) {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-border-primary/50 bg-bg-tertiary px-3 py-2">
      <Wrench size={13} className="mt-0.5 shrink-0 text-text-muted" />
      <div className="min-w-0 flex-1">
        <div className="text-[11px] text-text-muted">
          工具结果{message.toolCallId ? ` · ${message.toolCallId}` : ""}
        </div>
        <pre className="mt-1 max-h-40 overflow-auto text-xs text-text-secondary whitespace-pre-wrap break-all">
          {message.text}
        </pre>
      </div>
    </div>
  );
}

export function LogConversation({ normalized }: LogConversationProps) {
  const [showHidden, setShowHidden] = useState(false);
  const messages = normalized.messages.filter(
    (message) => showHidden || !message.hidden,
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-text-muted">
          默认隐藏系统指令、环境上下文与注入内容；原始 JSON 始终完整保留。
        </p>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => setShowHidden((value) => !value)}
        >
          {showHidden ? <EyeOff size={14} /> : <Eye size={14} />}
          {showHidden ? "隐藏系统消息" : "显示系统消息"}
        </Button>
      </div>

      {messages.length === 0 ? (
        <EmptyState
          icon={Bot}
          title="没有可展示的对话"
          description="该日志没有可解析的用户/助手消息"
        />
      ) : (
        <div className="space-y-4">
          {messages.map((message) => {
            if (message.hidden) {
              return (
                <div
                  key={message.id}
                  className="rounded-lg border border-dashed border-border-primary/60 bg-bg-primary/30 px-3 py-2"
                >
                  <div className="text-[11px] text-text-muted">
                    系统/环境消息{message.hiddenReason ? ` · ${message.hiddenReason}` : ""}
                  </div>
                  <pre className="mt-1 max-h-32 overflow-auto text-xs text-text-muted whitespace-pre-wrap break-all">
                    {message.text || "(空)"}
                  </pre>
                </div>
              );
            }
            if (message.role === "tool") {
              return <ToolRow key={message.id} message={message} />;
            }
            if (message.role === "system" || message.role === "developer") {
              return (
                <div
                  key={message.id}
                  className="rounded-lg border border-dashed border-border-primary/60 bg-bg-primary/30 px-3 py-2"
                >
                  <div className="text-[11px] text-text-muted">
                    {message.role === "system" ? "系统消息" : "开发者消息"}
                  </div>
                  <pre className="mt-1 max-h-32 overflow-auto text-xs text-text-muted whitespace-pre-wrap break-all">
                    {message.text}
                  </pre>
                </div>
              );
            }
            const isUser = message.role === "user";
            return (
              <div
                key={message.id}
                className={`flex gap-3 ${isUser ? "justify-end" : "justify-start"}`}
              >
                {!isUser && (
                  <div className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent/15">
                    <Bot size={16} className="text-accent" />
                  </div>
                )}
                <div
                  className={`max-w-[82%] rounded-xl px-4 py-3 ${
                    isUser
                      ? "bg-accent text-white"
                      : "bg-bg-tertiary border border-border-primary"
                  }`}
                >
                  {isUser ? (
                    <p className="text-sm leading-relaxed whitespace-pre-wrap">
                      {message.text}
                    </p>
                  ) : message.text.length > 60_000 ? (
                    <pre className="text-xs text-text-primary whitespace-pre-wrap break-all">
                      {message.text}
                    </pre>
                  ) : (
                    <div className="text-sm text-text-primary">
                      <MarkdownContent>{message.text}</MarkdownContent>
                    </div>
                  )}
                  <ThinkingBlock text={message.thinking} />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
