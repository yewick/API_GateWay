import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Upload, FileText, RotateCw, Trash2, Loader2, Eye } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import { EmptyState } from "../ui/EmptyState";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Spinner } from "../ui/Spinner";
import { DocumentViewerModal } from "./DocumentViewerModal";
import {
  useKbDocuments,
  useUploadDocument,
  useIngestDocument,
  useDeleteDocument,
  knowledgeKeys,
} from "../../hooks/useKnowledge";
import { docStatus, formatBytes, DOC_EXTENSIONS } from "../../lib/knowledge";
import { formatTime } from "../../lib/constants";
import { toast } from "../../lib/toast";
import type { KbDocument } from "../../types";

interface DocumentListProps {
  kbId: string;
}

export function DocumentList({ kbId }: DocumentListProps) {
  const qc = useQueryClient();
  const { data: docs = [], isLoading } = useKbDocuments(kbId);
  const uploadMutation = useUploadDocument();
  const ingestMutation = useIngestDocument();
  const deleteMutation = useDeleteDocument();

  const [deleting, setDeleting] = useState<KbDocument | null>(null);
  const [viewing, setViewing] = useState<KbDocument | null>(null);

  // 存在解析/入库中的文档时，轮询刷新（后台解析回填后自动更新状态）
  const hasPending = docs.some((d) =>
    ["parsing", "processing"].includes(d.status),
  );
  useEffect(() => {
    if (!hasPending) return;
    const timer = setInterval(() => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.documents(kbId) });
    }, 3000);
    return () => clearInterval(timer);
  }, [hasPending, kbId, qc]);

  const handleUpload = async () => {
    let selected: string | string[] | null = null;
    try {
      selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "文档", extensions: DOC_EXTENSIONS }],
      });
    } catch (err) {
      toast.error("选择文件失败", (err as Error)?.message);
      return;
    }
    if (!selected || Array.isArray(selected)) return;

    try {
      const res = await uploadMutation.mutateAsync({ kbId, path: selected });
      if (res.duplicate) {
        toast.info("已存在相同文档", `「${res.document.filename}」内容相同，已跳过`);
      } else {
        toast.success("上传成功", `「${res.document.filename}」已进入后台解析`);
      }
    } catch (err) {
      toast.error("上传失败", (err as Error)?.message);
    }
  };

  const handleIngest = async (doc: KbDocument) => {
    try {
      const res = await ingestMutation.mutateAsync({ kbId, docId: doc.id });
      toast.success("入库成功", `已生成 ${res.chunk_count} 个切片`);
    } catch (err) {
      toast.error("入库失败", (err as Error)?.message);
    }
  };

  const handleDelete = async () => {
    if (!deleting) return;
    try {
      await deleteMutation.mutateAsync({ kbId, docId: deleting.id });
      toast.success("删除成功", `文档「${deleting.filename}」已删除`);
      setDeleting(null);
    } catch (err) {
      toast.error("删除失败", (err as Error)?.message);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between mb-4 flex-shrink-0">
        <p className="text-xs text-text-muted">
          共 {docs.length} 个文档 · 上传后自动解析，解析完成需手动「入库」向量化
        </p>
        <Button onClick={handleUpload} loading={uploadMutation.isPending}>
          <Upload size={15} />
          上传文档
        </Button>
      </div>

      {isLoading ? (
        <div className="flex justify-center py-14">
          <Spinner />
        </div>
      ) : docs.length === 0 ? (
        <EmptyState
          icon={FileText}
          title="暂无文档"
          description="点击「上传文档」选择本地文件，解析后将进行分块与向量化"
          action={
            <Button variant="secondary" onClick={handleUpload}>
              <Upload size={15} />
              上传第一个文档
            </Button>
          }
        />
      ) : (
        <ul className="flex-1 min-h-0 overflow-y-auto divide-y divide-border-primary">
          {docs.map((doc) => {
            const st = docStatus(doc.status);
            const pending = ["parsing", "processing"].includes(doc.status);
            return (
              <li
                key={doc.id}
                className="py-3 flex items-center gap-4"
              >
                <div className="w-9 h-9 rounded-lg bg-bg-tertiary flex items-center justify-center flex-shrink-0">
                  <FileText size={16} className="text-text-muted" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-text-primary truncate">
                      {doc.filename}
                    </span>
                    {pending && <Loader2 size={13} className="animate-spin text-accent flex-shrink-0" />}
                    <Badge variant={st.variant}>{st.label}</Badge>
                  </div>
                  <div className="text-xs text-text-muted mt-1 tabular">
                    {doc.file_type || "未知类型"} · {formatBytes(doc.file_size)}
                    {doc.chunk_count > 0 && ` · ${doc.chunk_count} 切片`}
                    {doc.token_count > 0 && ` · ${doc.token_count} tokens`}
                    {" · "}{formatTime(doc.created_at)}
                  </div>
                  {doc.error_message && (
                    <p className="text-xs text-danger mt-1 truncate">{doc.error_message}</p>
                  )}
                </div>
                <div className="flex items-center gap-1.5 flex-shrink-0">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setViewing(doc)}
                    title="查看解析内容与切片"
                  >
                    <Eye size={13} />
                  </Button>
                  {doc.status === "awaiting_review" && (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleIngest(doc)}
                      loading={ingestMutation.isPending}
                      title="入库（分块 + 向量化）"
                    >
                      <RotateCw size={13} />
                      入库
                    </Button>
                  )}
                  <Button
                    variant="danger"
                    size="sm"
                    onClick={() => setDeleting(doc)}
                    title="删除文档"
                  >
                    <Trash2 size={13} />
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <ConfirmDialog
        open={!!deleting}
        title="删除文档"
        description={`确定要删除文档「${deleting?.filename ?? ""}」吗？其切片与向量将一并删除。`}
        confirmText="删除"
        danger
        loading={deleteMutation.isPending}
        onConfirm={handleDelete}
        onCancel={() => setDeleting(null)}
      />

      <DocumentViewerModal
        open={!!viewing}
        onClose={() => setViewing(null)}
        kbId={kbId}
        doc={viewing}
      />
    </div>
  );
}
