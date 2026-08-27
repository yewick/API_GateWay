import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { Components } from "react-markdown";
import { FileText } from "lucide-react";
import { Modal } from "../ui/Modal";
import { Spinner } from "../ui/Spinner";
import { Badge } from "../ui/Badge";
import { useKbDocumentContent, useKbDocumentChunks } from "../../hooks/useKnowledge";
import type { KbDocument } from "../../types";

interface DocumentViewerModalProps {
  open: boolean;
  onClose: () => void;
  kbId: string;
  doc: KbDocument | null;
}

/** Markdown 渲染样式（无 typography 插件，手工指定各元素样式） */
const mdComponents: Components = {
  h1: ({ children }) => (
    <h1 className="text-lg font-semibold text-text-primary mt-5 mb-2 first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-base font-semibold text-text-primary mt-5 mb-2 first:mt-0">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-sm font-semibold text-text-primary mt-4 mb-2 first:mt-0">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="text-sm font-medium text-text-primary mt-3 mb-1.5 first:mt-0">{children}</h4>
  ),
  p: ({ children }) => <p className="text-sm text-text-secondary leading-relaxed mb-3">{children}</p>,
  ul: ({ children }) => <ul className="list-disc pl-5 mb-3 space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal pl-5 mb-3 space-y-1">{children}</ol>,
  li: ({ children }) => <li className="text-sm text-text-secondary leading-relaxed">{children}</li>,
  a: ({ children, href }) => (
    <a href={href} target="_blank" rel="noreferrer" className="text-accent underline underline-offset-2">
      {children}
    </a>
  ),
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-border-primary pl-3 my-3 text-text-muted">{children}</blockquote>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto mb-3">
      <table className="w-full text-xs border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border-primary bg-bg-tertiary px-2 py-1.5 text-left font-medium text-text-primary">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-border-primary px-2 py-1.5 text-text-secondary">{children}</td>
  ),
  hr: () => <hr className="border-border-primary my-4" />,
  strong: ({ children }) => <strong className="font-semibold text-text-primary">{children}</strong>,
  code: ({ className, children, ...props }) => {
    const match = /language-(\w+)/.exec(className || "");
    const codeText = String(children).replace(/\n$/, "");
    if (match) {
      return (
        <SyntaxHighlighter style={oneDark} language={match[1]} PreTag="div" customStyle={{ borderRadius: 8, fontSize: 12, margin: "0 0 12px" }}>
          {codeText}
        </SyntaxHighlighter>
      );
    }
    return (
      <code className="px-1.5 py-0.5 rounded bg-bg-tertiary text-[13px] text-accent" {...props}>
        {children}
      </code>
    );
  },
};

/** 文档查看器：解析后的 Markdown 渲染 + 切片列表（核对分块效果用）。 */
export function DocumentViewerModal({ open, onClose, kbId, doc }: DocumentViewerModalProps) {
  const { data: content, isLoading: loadingContent } = useKbDocumentContent(
    open ? kbId : null,
    open ? (doc?.id ?? null) : null,
  );
  const { data: chunks = [], isLoading: loadingChunks } = useKbDocumentChunks(
    open ? kbId : null,
    open ? (doc?.id ?? null) : null,
  );

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={doc ? `文档查看：${doc.filename}` : "文档查看"}
      description="解析后的 Markdown 内容与切片结果"
      size="xl"
    >
      <div className="space-y-6">
        {/* 解析内容 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <FileText size={14} className="text-text-muted" />
            <h3 className="text-sm font-semibold text-text-primary">解析内容</h3>
            {content?.file_type && <Badge variant="neutral">{content.file_type}</Badge>}
          </div>
          {loadingContent ? (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          ) : content?.content ? (
            <div className="rounded-lg border border-border-primary bg-bg-primary p-4">
              <Markdown remarkPlugins={[remarkGfm]} components={mdComponents}>
                {content.content}
              </Markdown>
            </div>
          ) : (
            <p className="text-xs text-text-muted py-6 text-center">
              尚未解析或内容为空（解析完成后可在此预览）
            </p>
          )}
        </section>

        {/* 切片列表 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <h3 className="text-sm font-semibold text-text-primary">切片</h3>
            <span className="text-xs text-text-muted tabular">共 {chunks.length} 条</span>
          </div>
          {loadingChunks ? (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          ) : chunks.length === 0 ? (
            <p className="text-xs text-text-muted py-6 text-center">
              尚未入库，切片为空（点击「入库」后生成）
            </p>
          ) : (
            <ul className="space-y-2">
              {chunks.map((c) => (
                <li
                  key={c.chunk_index}
                  className="rounded-lg border border-border-primary bg-bg-primary p-3"
                >
                  <div className="flex items-center gap-2 mb-1.5">
                    <span className="text-[10px] font-medium text-text-muted tabular">
                      #{c.chunk_index + 1}
                    </span>
                    <span className="text-[10px] text-text-muted tabular">{c.token_count} tokens</span>
                    {c.symbol_name && (
                      <Badge variant="info">
                        {c.symbol_kind ?? "symbol"} {c.symbol_name}
                      </Badge>
                    )}
                  </div>
                  <p className="text-xs text-text-secondary leading-relaxed whitespace-pre-wrap break-words">
                    {c.content}
                  </p>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </Modal>
  );
}
