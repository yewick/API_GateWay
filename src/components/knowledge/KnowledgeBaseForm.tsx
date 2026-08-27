import { useEffect, useState } from "react";
import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { useCreateKnowledgeBase } from "../../hooks/useKnowledge";
import { useChannels } from "../../hooks/useChannels";
import { toast } from "../../lib/toast";
import type { KbKnowledgeBase } from "../../types";

interface KnowledgeBaseFormProps {
  open: boolean;
  onClose: () => void;
  onCreated?: (kb: KbKnowledgeBase) => void;
}

/**
 * 创建知识库（Phase A）。
 * 名称必填；embedding 模型/渠道可选（留空时后端按默认配置校验/回退）。
 * 分块/排除目录等高级参数属于 Phase B，此处不展开。
 */
export function KnowledgeBaseForm({ open, onClose, onCreated }: KnowledgeBaseFormProps) {
  const createMutation = useCreateKnowledgeBase();
  const { data: channels } = useChannels();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [embeddingModel, setEmbeddingModel] = useState("");
  const [embeddingChannelId, setEmbeddingChannelId] = useState("");

  useEffect(() => {
    if (open) {
      setName("");
      setDescription("");
      setEmbeddingModel("");
      setEmbeddingChannelId("");
    }
  }, [open]);

  const activeChannels = (channels ?? []).filter((c) => c.status === 1);

  const submit = async () => {
    if (!name.trim()) {
      toast.warning("缺少名称", "请输入知识库名称");
      return;
    }
    try {
      const kb = await createMutation.mutateAsync({
        name: name.trim(),
        description: description.trim() || null,
        embedding_model: embeddingModel.trim() || null,
        embedding_channel_id: embeddingChannelId || null,
      });
      toast.success("创建成功", `知识库「${kb.name}」已创建`);
      onCreated?.(kb);
      onClose();
    } catch (err) {
      toast.error("创建失败", (err as Error)?.message);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="创建知识库"
      description="用于 RAG 检索与问答的向量知识库"
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={createMutation.isPending}>
            取消
          </Button>
          <Button onClick={submit} loading={createMutation.isPending}>
            创建
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Input
          label="名称"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="例如：产品文档"
          autoFocus
        />
        <Input
          label="描述"
          textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="可选，简要说明该知识库的用途"
        />
        <Input
          label="Embedding 模型"
          value={embeddingModel}
          onChange={(e) => setEmbeddingModel(e.target.value)}
          placeholder="可选，留空使用默认模型（如 text-embedding-3-small）"
          hint="用于向量化的 embedding 模型名"
        />
        <Select
          label="Embedding 渠道"
          options={[
            { value: "", label: "自动选择（默认）" },
            ...activeChannels.map((c) => ({ value: c.id, label: c.name })),
          ]}
          value={embeddingChannelId}
          onChange={(e) => setEmbeddingChannelId(e.target.value)}
          hint="可选，指定用于向量化的渠道"
        />
      </div>
    </Modal>
  );
}
