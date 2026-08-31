import { useState } from "react";
import { Search, FileText } from "lucide-react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { EmptyState } from "../ui/EmptyState";
import { Spinner } from "../ui/Spinner";
import { useSearchKb } from "../../hooks/useKnowledge";
import { toast } from "../../lib/toast";
import type { SearchResult } from "../../types";

interface SearchPanelProps {
  kbId: string;
}

const TOP_K_OPTIONS = [
  { value: "3", label: "Top 3" },
  { value: "5", label: "Top 5" },
  { value: "10", label: "Top 10" },
  { value: "20", label: "Top 20" },
];

export function SearchPanel({ kbId }: SearchPanelProps) {
  const searchMutation = useSearchKb();
  const [query, setQuery] = useState("");
  const [topK, setTopK] = useState("5");
  const [results, setResults] = useState<SearchResult[] | null>(null);

  const runSearch = async () => {
    if (!query.trim()) {
      toast.warning("缺少关键词", "请输入检索内容");
      return;
    }
    try {
      const res = await searchMutation.mutateAsync({
        kbId,
        query: query.trim(),
        topK: Number(topK),
      });
      setResults(res);
    } catch (err) {
      toast.error("检索失败", (err as Error)?.message);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-end gap-3 mb-5 flex-shrink-0">
        <div className="flex-1">
          <Input
            label="混合检索"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="输入问题或关键词，向量 + 关键词混合召回"
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing || e.keyCode === 229) return;
              if (e.key === "Enter") runSearch();
            }}
          />
        </div>
        <div className="w-28">
          <Select
            label="返回条数"
            options={TOP_K_OPTIONS}
            value={topK}
            onChange={(e) => setTopK(e.target.value)}
          />
        </div>
        <Button onClick={runSearch} loading={searchMutation.isPending}>
          <Search size={15} />
          检索
        </Button>
      </div>

      {searchMutation.isPending ? (
        <div className="flex justify-center py-14">
          <Spinner />
        </div>
      ) : results === null ? (
        <EmptyState
          icon={Search}
          title="尚未检索"
          description="输入查询内容，将返回最相关的文档片段与相关度得分"
        />
      ) : results.length === 0 ? (
        <EmptyState icon={Search} title="无匹配结果" description="未找到相关内容，尝试更换关键词" />
      ) : (
        <ul className="flex-1 min-h-0 overflow-y-auto space-y-3">
          {results.map((r, i) => (
            <li
              key={r.chunk_id}
              className="p-4 rounded-lg bg-bg-tertiary border border-border-primary"
            >
              <div className="flex items-center gap-2 mb-2">
                <span className="text-[10px] font-medium text-text-muted tabular">
                  #{i + 1}
                </span>
                <FileText size={13} className="text-text-muted" />
                <span className="text-xs font-medium text-text-primary truncate">
                  {r.filename}
                </span>
                <span className="ml-auto text-xs text-accent tabular">
                  {r.score.toFixed(3)}
                </span>
              </div>
              <p className="text-xs text-text-secondary leading-relaxed whitespace-pre-wrap">
                {r.content}
              </p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
