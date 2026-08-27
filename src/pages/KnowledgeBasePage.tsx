import { useEffect, useState } from "react";
import {
  BookOpen,
  Plus,
  FileText,
  Bot,
  MessageSquare,
  Search,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Card } from "../components/ui/Card";
import { Tabs, type TabItem } from "../components/ui/Tabs";
import { EmptyState } from "../components/ui/EmptyState";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { Spinner } from "../components/ui/Spinner";
import { KnowledgeBaseForm } from "../components/knowledge/KnowledgeBaseForm";
import { DocumentList } from "../components/knowledge/DocumentList";
import { AskPanel } from "../components/knowledge/AskPanel";
import { ConversationPanel } from "../components/knowledge/ConversationPanel";
import { SearchPanel } from "../components/knowledge/SearchPanel";
import { KbInfoPopover } from "../components/knowledge/KbInfoPopover";
import {
  useKnowledgeBases,
  useDeleteKnowledgeBase,
  useUpdateKnowledgeBase,
} from "../hooks/useKnowledge";
import { indexStatus } from "../lib/knowledge";
import { toast } from "../lib/toast";
import type { KbKnowledgeBase } from "../types";

type DetailTab = "ask" | "documents" | "conversations" | "search";

// 以问答（聊天）为主，其余为次级 Tab
const DETAIL_TABS: TabItem[] = [
  { key: "ask", label: "问答", icon: <Bot size={15} /> },
  { key: "documents", label: "文档", icon: <FileText size={15} /> },
  { key: "search", label: "检索", icon: <Search size={15} /> },
  { key: "conversations", label: "记录", icon: <MessageSquare size={15} /> },
];

export const KnowledgeBasePage = () => {
  const { data: kbs = [], isLoading } = useKnowledgeBases();
  const deleteMutation = useDeleteKnowledgeBase();
  const updateMutation = useUpdateKnowledgeBase();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<DetailTab>("ask");
  const [formOpen, setFormOpen] = useState(false);
  const [deleting, setDeleting] = useState<KbKnowledgeBase | null>(null);
  const [kbListCollapsed, setKbListCollapsed] = useState(false);

  const selected = kbs.find((k) => k.id === selectedId) ?? null;

  // 首次加载自动选中第一个知识库
  useEffect(() => {
    if (!selectedId && kbs.length > 0) {
      setSelectedId(kbs[0].id);
    }
  }, [kbs, selectedId]);

  // 当前选中项被删除后回退
  useEffect(() => {
    if (selectedId && kbs.length > 0 && !kbs.some((k) => k.id === selectedId)) {
      setSelectedId(kbs[0].id);
    }
  }, [kbs, selectedId]);

  const handleDelete = async () => {
    if (!deleting) return;
    try {
      await deleteMutation.mutateAsync(deleting.id);
      toast.success("删除成功", `知识库「${deleting.name}」已删除`);
      setDeleting(null);
      if (selectedId === deleting.id) setSelectedId(null);
    } catch (err) {
      toast.error("删除失败", (err as Error)?.message);
    }
  };

  const toggleMcp = async (kb: KbKnowledgeBase) => {
    const next = kb.mcp_enabled === 1 ? 0 : 1;
    try {
      await updateMutation.mutateAsync({ id: kb.id, input: { mcp_enabled: next } });
      toast.success(next === 1 ? "已启用 MCP" : "已禁用 MCP", `知识库「${kb.name}」`);
    } catch (err) {
      toast.error("操作失败", (err as Error)?.message);
    }
  };

  return (
    <div>
      <PageHeader
        title="知识库"
        description="RAG 检索增强：文档向量化、混合检索与带来源的问答"
        actions={
          <Button onClick={() => setFormOpen(true)}>
            <Plus size={16} />
            创建知识库
          </Button>
        }
      />

      <div
        className={`grid grid-cols-1 gap-5 items-start ${
          kbListCollapsed ? "lg:grid-cols-[48px_1fr]" : "lg:grid-cols-[280px_1fr]"
        }`}
      >
        {/* 左侧：知识库列表（可折叠） */}
        {kbListCollapsed ? (
          <div className="lg:sticky lg:top-4 flex justify-center">
            <button
              onClick={() => setKbListCollapsed(false)}
              className="p-2 rounded-lg bg-bg-secondary border border-border-primary text-text-muted hover:text-text-primary transition-colors"
              title="展开知识库列表"
              aria-label="展开知识库列表"
            >
              <PanelLeftOpen size={16} />
            </button>
          </div>
        ) : (
          <Card noPadding className="lg:sticky lg:top-4">
            <div className="flex items-center justify-between px-3 pt-2">
              <span className="text-xs font-medium text-text-muted">知识库</span>
              <button
                onClick={() => setKbListCollapsed(true)}
                className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
                title="折叠知识库列表"
                aria-label="折叠知识库列表"
              >
                <PanelLeftClose size={16} />
              </button>
            </div>
            {isLoading ? (
              <div className="flex justify-center py-14">
                <Spinner />
              </div>
            ) : kbs.length === 0 ? (
              <EmptyState
                icon={BookOpen}
                title="暂无知识库"
                description="创建知识库后即可上传文档并开始 RAG 问答"
              />
            ) : (
              <ul className="p-2 space-y-1">
                {kbs.map((kb) => {
                  const idx = indexStatus(kb.index_status);
                  const active = kb.id === selectedId;
                  return (
                    <li key={kb.id}>
                      <button
                        onClick={() => {
                          setSelectedId(kb.id);
                          setActiveTab("ask");
                        }}
                        className={`w-full text-left rounded-lg px-3 py-2.5 transition-colors ${
                          active
                            ? "bg-accent/10 border border-accent/30"
                            : "hover:bg-bg-hover border border-transparent"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <BookOpen
                            size={15}
                            className={active ? "text-accent" : "text-text-muted"}
                          />
                          <span className="text-sm font-medium text-text-primary truncate">
                            {kb.name}
                          </span>
                        </div>
                        <div className="flex items-center gap-1.5 mt-1.5">
                          <span className="text-[11px] text-text-muted tabular">
                            {kb.doc_count} 文档
                          </span>
                          <Badge variant={idx.variant} className="!px-1.5 !py-0">
                            {idx.label}
                          </Badge>
                        </div>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </Card>
        )}

        {/* 右侧：聊天为主 */}
        <div className="flex flex-col gap-4 lg:h-[calc(100vh-140px)] min-h-0">
          {!selected ? (
            <Card>
              <EmptyState
                icon={BookOpen}
                title="选择一个知识库"
                description="从左侧列表选择知识库，查看问答、文档与检索"
                action={
                  <Button variant="secondary" onClick={() => setFormOpen(true)}>
                    <Plus size={15} />
                    新建知识库
                  </Button>
                }
              />
            </Card>
          ) : (
            <>
              {/* 顶部细条：名称 + 信息气泡 */}
              <Card className="flex-shrink-0">
                <div className="flex items-center justify-between gap-4">
                  <div className="min-w-0 flex items-center gap-2">
                    <h2 className="text-base font-semibold text-text-primary truncate">
                      {selected.name}
                    </h2>
                    <Badge variant="neutral">{selected.embedding_model ?? "默认模型"}</Badge>
                  </div>
                  <KbInfoPopover
                    kb={selected}
                    onToggleMcp={toggleMcp}
                    onDelete={(kb) => setDeleting(kb)}
                  />
                </div>
                {selected.description && (
                  <p className="text-xs text-text-secondary mt-1">{selected.description}</p>
                )}
              </Card>

              {/* 功能标签页（填满剩余高度） */}
              <Card noPadding className="flex-1 min-h-0 flex flex-col">
                <div className="px-4 pt-3 flex-shrink-0">
                  <Tabs
                    tabs={DETAIL_TABS}
                    activeKey={activeTab}
                    onChange={(k) => setActiveTab(k as DetailTab)}
                  />
                </div>
                <div className="flex-1 min-h-0 p-5 flex flex-col">
                  {activeTab === "ask" && <AskPanel kbId={selected.id} />}
                  {activeTab === "documents" && <DocumentList kbId={selected.id} />}
                  {activeTab === "conversations" && <ConversationPanel kbId={selected.id} />}
                  {activeTab === "search" && <SearchPanel kbId={selected.id} />}
                </div>
              </Card>

              {/* Phase B 待办提示 */}
              <p className="text-[11px] text-text-muted leading-relaxed flex-shrink-0">
                更多能力（索引管理、多源导入 git/url/本地目录、文档内容查看、FTS 关键词与符号过滤、分块参数编辑）将在后续版本提供。
              </p>
            </>
          )}
        </div>
      </div>

      {/* 创建弹窗 */}
      <KnowledgeBaseForm
        open={formOpen}
        onClose={() => setFormOpen(false)}
        onCreated={(kb) => setSelectedId(kb.id)}
      />

      {/* 删除确认 */}
      <ConfirmDialog
        open={!!deleting}
        title="删除知识库"
        description={`确定要删除知识库「${deleting?.name ?? ""}」吗？其全部文档、切片与向量将一并删除。`}
        confirmText="删除"
        danger
        loading={deleteMutation.isPending}
        onConfirm={handleDelete}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
};
