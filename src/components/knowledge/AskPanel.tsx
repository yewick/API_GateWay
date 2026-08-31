import { useEffect, useRef, useState } from "react";
import { Send, FileText, User, Bot, Loader2, ChevronDown, ChevronRight, SlidersHorizontal } from "lucide-react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Slider } from "../ui/Slider";
import { EmptyState } from "../ui/EmptyState";
import { MarkdownContent } from "../ui/Markdown";
import { useAskKnowledgeBase } from "../../hooks/useKnowledge";
import { useApiKeys } from "../../hooks/useApiKeys";
import type { RagUsage, SearchResult, RetrievalDetail } from "../../types";

interface AskPanelProps {
  kbId: string;
}

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  sources?: SearchResult[];
  usage?: RagUsage | null;
  retrievalDetails?: RetrievalDetail[] | null;
}

const TOP_K_OPTIONS = [
  { value: "3", label: "Top 3" },
  { value: "5", label: "Top 5" },
  { value: "10", label: "Top 10" },
  { value: "20", label: "Top 20" },
];

const SEARCH_MODE_OPTIONS = [
  { value: "hybrid", label: "混合（hybrid）" },
  { value: "vector", label: "向量（vector）" },
  { value: "keyword", label: "关键词（keyword）" },
];

export function AskPanel({ kbId }: AskPanelProps) {
  const askMutation = useAskKnowledgeBase();
  const { data: apiKeys } = useApiKeys();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [question, setQuestion] = useState("");
  const [apiKeyId, setApiKeyId] = useState("");
  const [model, setModel] = useState("");
  const [topK, setTopK] = useState("5");
  const [expandedSources, setExpandedSources] = useState<Set<number>>(new Set());
  const [searchMode, setSearchMode] = useState("hybrid");
  const [vectorWeight, setVectorWeight] = useState(0.7);
  const [keywordWeight, setKeywordWeight] = useState(0.3);
  const [showSearchConfig, setShowSearchConfig] = useState(false);
  const [expandedDetails, setExpandedDetails] = useState<Set<number>>(new Set());
  const scrollRef = useRef<HTMLDivElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  const enabledKeys = (apiKeys ?? []).filter((k) => k.status === 1);
  const selectedKey = enabledKeys.find((k) => k.id === apiKeyId);
  const modelOptions = selectedKey?.allowed_models ?? [];

  const keyOptions = [
    { value: "", label: "自动选择" },
    ...enabledKeys.map((k) => ({ value: k.id, label: k.name })),
  ];

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages, askMutation.isPending]);

  const autoGrow = () => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  };

  const handleKeyChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const id = e.target.value;
    setApiKeyId(id);
    const k = enabledKeys.find((x) => x.id === id);
    setModel(k?.allowed_models?.[0] ?? "");
  };

  const toggleSources = (i: number) => {
    setExpandedSources((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  };

  const toggleDetails = (i: number) => {
    setExpandedDetails((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  };

  // 向量/关键词权重恒等于 1：拖动任一滑块，另一个自动互补。
  const handleVectorWeight = (v: number) => {
    setVectorWeight(v);
    setKeywordWeight(Number((1 - v).toFixed(2)));
  };

  const handleKeywordWeight = (v: number) => {
    setKeywordWeight(v);
    setVectorWeight(Number((1 - v).toFixed(2)));
  };

  const send = async () => {
    if (!question.trim()) return;
    const q = question.trim();
    setQuestion("");
    if (taRef.current) taRef.current.style.height = "auto";

    const history = messages
      .filter((m) => m.content.trim())
      .map((m) => ({ role: m.role, content: m.content }));

    setMessages((prev) => [...prev, { role: "user", content: q }]);

    try {
      const res = await askMutation.mutateAsync({
        kbId,
        question: q,
        options: {
          model: model.trim() || undefined,
          topK: Number(topK),
          history,
          apiKeyId: apiKeyId || undefined,
          searchMode,
          ...(searchMode === "hybrid" ? { vectorWeight, keywordWeight } : {}),
        },
      });
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: res.answer,
          sources: res.sources,
          usage: res.usage,
          retrievalDetails: res.retrieval_details,
        },
      ]);
    } catch (err) {
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: `⚠️ 问答失败：${(err as Error)?.message ?? "未知错误"}`,
        },
      ]);
    }
  };

  return (
    <div className="flex flex-col h-full min-h-0">
      {messages.length === 0 ? (
        <div className="flex-1 flex items-center justify-center min-h-[200px]">
          <EmptyState
            icon={Bot}
            title="基于知识库问答"
            description="输入问题，将检索相关文档片段并由模型生成带来源的回答，支持多轮追问"
          />
        </div>
      ) : (
        <div
          ref={scrollRef}
          className="flex-1 min-h-0 space-y-4 overflow-y-auto pr-1"
        >
          {messages.map((msg, i) => (
            <div
              key={i}
              className={`flex gap-3 ${msg.role === "user" ? "justify-end" : "justify-start"}`}
            >
              {msg.role === "assistant" && (
                <div className="w-8 h-8 rounded-lg bg-accent/15 flex items-center justify-center flex-shrink-0">
                  <Bot size={16} className="text-accent" />
                </div>
              )}
              <div
                className={`max-w-[80%] min-w-0 rounded-xl px-4 py-3 ${
                  msg.role === "user"
                    ? "bg-accent text-white"
                    : "bg-bg-tertiary border border-border-primary"
                }`}
              >
                {msg.role === "assistant" ? (
                  <MarkdownContent>{msg.content}</MarkdownContent>
                ) : (
                  <p className="text-sm leading-relaxed whitespace-pre-wrap">
                    {msg.content}
                  </p>
                )}

                {msg.sources && msg.sources.length > 0 && (
                  <div className="mt-3 pt-3 border-t border-border-primary/60">
                    <button
                      type="button"
                      onClick={() => toggleSources(i)}
                      className="flex items-center gap-1.5 w-full text-left text-[11px] font-medium text-text-muted hover:text-text-primary transition-colors"
                    >
                      {expandedSources.has(i) ? (
                        <ChevronDown size={12} className="flex-shrink-0" />
                      ) : (
                        <ChevronRight size={12} className="flex-shrink-0" />
                      )}
                      引用来源（{msg.sources.length}）
                    </button>
                    {expandedSources.has(i) && (
                      <div className="mt-2 space-y-2">
                        {msg.sources.map((s) => (
                          <div key={s.chunk_id} className="flex items-start gap-2">
                            <FileText size={12} className="text-text-muted flex-shrink-0 mt-0.5" />
                            <div className="min-w-0">
                              <span className="text-[11px] text-text-secondary truncate block">
                                {s.filename} · {s.score.toFixed(3)}
                              </span>
                              <p className="text-[11px] text-text-muted leading-relaxed line-clamp-2">
                                {s.content}
                              </p>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {msg.retrievalDetails && msg.retrievalDetails.length > 0 && (
                  <div className="mt-3 pt-3 border-t border-border-primary/60">
                    <button
                      type="button"
                      onClick={() => toggleDetails(i)}
                      className="flex items-center gap-1.5 w-full text-left text-[11px] font-medium text-text-muted hover:text-text-primary transition-colors"
                    >
                      {expandedDetails.has(i) ? (
                        <ChevronDown size={12} className="flex-shrink-0" />
                      ) : (
                        <ChevronRight size={12} className="flex-shrink-0" />
                      )}
                      查看检索明细（{msg.retrievalDetails.length}）
                    </button>
                    {expandedDetails.has(i) && (
                      <div className="mt-2 space-y-2">
                        {msg.retrievalDetails.map((d) => (
                          <div key={d.chunk_id} className="rounded-md bg-bg-primary/60 px-2.5 py-2">
                            <div className="flex items-center gap-2 flex-wrap text-[10px] text-text-muted tabular">
                              <FileText size={11} className="flex-shrink-0" />
                              <span className="text-text-secondary truncate max-w-[160px] min-w-0">{d.filename}</span>
                              <span>综合 {d.score.toFixed(3)}</span>
                              {d.vector_score != null && <span>向量 {d.vector_score.toFixed(3)}</span>}
                              {d.keyword_score != null && <span>关键词 {d.keyword_score.toFixed(3)}</span>}
                              {d.symbol_name && (
                                <span className="text-info break-all min-w-0">
                                  {d.symbol_name}
                                  {d.symbol_kind ? ` · ${d.symbol_kind}` : ""}
                                </span>
                              )}
                            </div>
                            <p className="text-[11px] text-text-muted leading-relaxed line-clamp-2 break-words mt-1">
                              {d.snippet}
                            </p>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {msg.usage && (
                  <p className="text-[10px] text-text-muted mt-2 tabular">
                    {msg.usage.prompt_tokens} prompt + {msg.usage.completion_tokens} completion tokens
                  </p>
                )}
              </div>
              {msg.role === "user" && (
                <div className="w-8 h-8 rounded-lg bg-bg-tertiary flex items-center justify-center flex-shrink-0">
                  <User size={16} className="text-text-muted" />
                </div>
              )}
            </div>
          ))}

          {askMutation.isPending && (
            <div className="flex items-center gap-3 justify-start">
              <div className="w-8 h-8 rounded-lg bg-accent/15 flex items-center justify-center flex-shrink-0">
                <Bot size={16} className="text-accent" />
              </div>
              <div className="flex items-center gap-2 text-sm text-text-muted px-4 py-3">
                <Loader2 size={14} className="animate-spin" />
                检索并生成中…
              </div>
            </div>
          )}
        </div>
      )}

      <div className="pt-4 border-t border-border-primary flex-shrink-0">
        <div className="flex items-end gap-3 mb-3 flex-wrap">
          <div className="w-52">
            <Select
              label="API Key"
              options={keyOptions}
              value={apiKeyId}
              onChange={handleKeyChange}
            />
          </div>
          <div className="w-44">
            {modelOptions.length > 0 ? (
              <Select
                label="对话模型"
                options={modelOptions.map((m) => ({ value: m, label: m }))}
                value={model}
                onChange={(e) => setModel(e.target.value)}
              />
            ) : (
              <Input
                label="对话模型"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="默认 gpt-4o"
              />
            )}
          </div>
          <div className="w-28">
            <Select
              label="检索条数"
              options={TOP_K_OPTIONS}
              value={topK}
              onChange={(e) => setTopK(e.target.value)}
            />
          </div>
          <div className="relative">
            <Button
              variant="secondary"
              onClick={() => setShowSearchConfig((v) => !v)}
              className="h-9"
            >
              <SlidersHorizontal size={15} />
              检索配置
            </Button>
            {showSearchConfig && (
              <div className="absolute bottom-full left-0 mb-2 z-20 w-72 max-h-[60vh] overflow-y-auto p-3 border border-border-primary rounded-lg bg-bg-secondary shadow-lg space-y-3">
                <Select
                  label="检索模式"
                  options={SEARCH_MODE_OPTIONS}
                  value={searchMode}
                  onChange={(e) => setSearchMode(e.target.value)}
                />
                {searchMode === "hybrid" && (
                  <div className="space-y-3">
                    <Slider
                      label="向量权重"
                      value={vectorWeight}
                      min={0}
                      max={1}
                      step={0.05}
                      onChange={handleVectorWeight}
                    />
                    <Slider
                      label="关键词权重"
                      value={keywordWeight}
                      min={0}
                      max={1}
                      step={0.05}
                      onChange={handleKeywordWeight}
                    />
                    <p className="text-[11px] text-text-muted">
                      向量 + 关键词权重恒等于 1，拖动任一滑块自动互补
                    </p>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        <div className="flex items-end gap-2">
          <textarea
            ref={taRef}
            rows={1}
            value={question}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => {
              setQuestion(e.target.value);
              autoGrow();
            }}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing || e.keyCode === 229) return;
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            placeholder="向知识库提问…（Enter 发送 / Shift+Enter 换行）"
            className="flex-1 px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary placeholder-text-muted outline-none transition-colors focus:border-accent focus:ring-1 focus:ring-accent/40 resize-none max-h-[200px] overflow-y-auto"
          />
          <Button onClick={send} disabled={!question.trim()} loading={askMutation.isPending}>
            <Send size={15} />
            发送
          </Button>
        </div>
      </div>
    </div>
  );
}
