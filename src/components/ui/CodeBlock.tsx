import { useState } from "react";
import { Check, ChevronDown, ChevronRight, Copy } from "lucide-react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import { useTheme } from "../../hooks/useTheme";
import { toast } from "../../lib/toast";

interface CodeBlockProps {
  code: string;
  title?: string;
  language?: string;
  defaultOpen?: boolean;
  maxHeight?: number;
  emptyText?: string;
}

export function CodeBlock({
  code,
  title,
  language = "json",
  defaultOpen,
  maxHeight = 360,
  emptyText = "无内容",
}: CodeBlockProps) {
  const { isDark } = useTheme();
  const [open, setOpen] = useState(defaultOpen ?? code.length <= 20_000);
  const [copied, setCopied] = useState(false);

  if (!code) {
    return (
      <div className="rounded-lg border border-border-primary bg-bg-tertiary px-3 py-2 text-xs text-text-muted">
        {emptyText}
      </div>
    );
  }

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      toast.success("已复制", "内容已复制到剪贴板");
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("复制失败", "请手动选择并复制");
    }
  };

  return (
    <div className="rounded-lg border border-border-primary overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 bg-bg-tertiary border-b border-border-primary/60">
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          className="flex items-center gap-1 text-xs font-medium text-text-secondary hover:text-text-primary"
        >
          {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          {title ?? language.toUpperCase()}
        </button>
        <span className="text-[10px] text-text-muted">
          {code.length.toLocaleString()} 字符
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={copy}
          className="flex items-center gap-1 rounded-md px-1.5 py-1 text-[11px] text-text-muted hover:text-text-primary hover:bg-bg-hover"
        >
          {copied ? <Check size={12} /> : <Copy size={12} />}
          复制
        </button>
      </div>
      {open ? (
        <div className="overflow-auto" style={{ maxHeight }}>
          <SyntaxHighlighter
            language={language}
            style={isDark ? oneDark : oneLight}
            customStyle={{
              margin: 0,
              borderRadius: 0,
              fontSize: "12px",
              background: "var(--bg-tertiary)",
            }}
            wrapLongLines
          >
            {code}
          </SyntaxHighlighter>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="w-full px-3 py-3 text-left text-xs text-text-muted hover:bg-bg-hover"
        >
          内容较大，已折叠。点击展开…
        </button>
      )}
    </div>
  );
}
