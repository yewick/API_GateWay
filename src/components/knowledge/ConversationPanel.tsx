import { useState } from "react";
import { MessageSquare, Trash2, User, Bot } from "lucide-react";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Spinner } from "../ui/Spinner";
import {
  useKbConversations,
  useClearConversations,
} from "../../hooks/useKnowledge";
import { formatTime } from "../../lib/constants";
import { toast } from "../../lib/toast";

interface ConversationPanelProps {
  kbId: string;
}

function parseSourceCount(sources: string | null): number {
  if (!sources) return 0;
  try {
    const arr = JSON.parse(sources);
    return Array.isArray(arr) ? arr.length : 0;
  } catch {
    return 0;
  }
}

export function ConversationPanel({ kbId }: ConversationPanelProps) {
  const { data: conversations = [], isLoading } = useKbConversations(kbId);
  const clearMutation = useClearConversations();
  const [confirmClear, setConfirmClear] = useState(false);

  const handleClear = async () => {
    try {
      await clearMutation.mutateAsync(kbId);
      toast.success("已清除", "对话记录已清空");
      setConfirmClear(false);
    } catch (err) {
      toast.error("清除失败", (err as Error)?.message);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between mb-4 flex-shrink-0">
        <p className="text-xs text-text-muted">共 {conversations.length} 条对话记录</p>
        {conversations.length > 0 && (
          <Button variant="danger" size="sm" onClick={() => setConfirmClear(true)}>
            <Trash2 size={13} />
            清空对话
          </Button>
        )}
      </div>

      {isLoading ? (
        <div className="flex justify-center py-14">
          <Spinner />
        </div>
      ) : conversations.length === 0 ? (
        <EmptyState
          icon={MessageSquare}
          title="暂无对话"
          description="在「问答」中向知识库提问后，对话记录会保存在这里"
        />
      ) : (
        <ul className="flex-1 min-h-0 overflow-y-auto space-y-3">
          {conversations.map((c) => {
            const isUser = c.role === "user";
            const srcCount = parseSourceCount(c.sources);
            return (
              <li
                key={c.id}
                className={`flex gap-3 ${isUser ? "justify-end" : "justify-start"}`}
              >
                {!isUser && (
                  <div className="w-8 h-8 rounded-lg bg-accent/15 flex items-center justify-center flex-shrink-0">
                    <Bot size={16} className="text-accent" />
                  </div>
                )}
                <div
                  className={`max-w-[80%] rounded-xl px-4 py-3 ${
                    isUser
                      ? "bg-accent text-white"
                      : "bg-bg-tertiary border border-border-primary"
                  }`}
                >
                  <p className="text-sm leading-relaxed whitespace-pre-wrap">{c.content}</p>
                  <p
                    className={`text-[10px] mt-2 tabular ${
                      isUser ? "text-white/70" : "text-text-muted"
                    }`}
                  >
                    {c.model ?? ""}
                    {srcCount > 0 && ` · ${srcCount} 来源`}
                    {c.tokens_used > 0 && ` · ${c.tokens_used} tokens`}
                    {" · "}
                    {formatTime(c.created_at)}
                  </p>
                </div>
                {isUser && (
                  <div className="w-8 h-8 rounded-lg bg-bg-tertiary flex items-center justify-center flex-shrink-0">
                    <User size={16} className="text-text-muted" />
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <ConfirmDialog
        open={confirmClear}
        title="清空对话"
        description="确定要清空该知识库的全部对话记录吗？此操作不可恢复。"
        confirmText="清空"
        danger
        loading={clearMutation.isPending}
        onConfirm={handleClear}
        onCancel={() => setConfirmClear(false)}
      />
    </div>
  );
}
