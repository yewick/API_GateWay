import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "../ui/Button";
import { toast } from "../../lib/toast";

interface ConfigPreviewProps {
  path: string;
  snippet: string;
  loading?: boolean;
}

export function ConfigPreview({ path, snippet, loading = false }: ConfigPreviewProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (!snippet) return;
    try {
      await navigator.clipboard.writeText(snippet);
      setCopied(true);
      toast.success("已复制", "配置片段已复制到剪贴板");
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("复制失败", "请手动选择并复制");
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-text-muted mono break-all">{path}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={copy}
          disabled={loading || !snippet}
          className="flex-shrink-0"
        >
          {copied ? <Check size={13} /> : <Copy size={13} />}
          复制
        </Button>
      </div>
      <pre className="text-xs mono text-text-primary bg-bg-tertiary rounded-lg p-3 overflow-x-auto max-h-64 overflow-y-auto whitespace-pre-wrap break-all">
        {loading ? "生成预览…" : snippet || "选择模型后显示配置预览"}
      </pre>
    </div>
  );
}
