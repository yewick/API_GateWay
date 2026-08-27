import { FileText } from "lucide-react";
import { Modal } from "../ui/Modal";
import { Spinner } from "../ui/Spinner";
import { Badge } from "../ui/Badge";
import { MarkdownContent } from "../ui/Markdown";
import { useKbDocumentContent, useKbDocumentChunks } from "../../hooks/useKnowledge";
import type { KbDocument } from "../../types";

interface DocumentViewerModalProps {
  open: boolean;
  onClose: () => void;
  kbId: string;
  doc: KbDocument | null;
}

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
              <MarkdownContent>{content.content}</MarkdownContent>
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
