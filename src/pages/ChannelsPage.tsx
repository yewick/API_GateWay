import { useState } from "react";
import { Plus, Pencil, Trash2, Power, PowerOff } from "lucide-react";
import type { Channel } from "../types";
import { useChannels, useDeleteChannel, useToggleChannel } from "../hooks/useChannels";
import { PageHeader } from "../components/ui/PageHeader";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Table, type Column } from "../components/ui/Table";
import { Card } from "../components/ui/Card";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { ChannelForm } from "../components/channels/ChannelForm";
import { ChannelTestButton } from "../components/channels/ChannelTestButton";
import { getChannelType, formatTime } from "../lib/constants";
import { Tooltip } from "../components/ui/Tooltip";
import { toast } from "../lib/toast";

export const ChannelsPage = () => {
  const { data: channels, isLoading } = useChannels();
  const deleteMutation = useDeleteChannel();
  const toggleMutation = useToggleChannel();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<Channel | null>(null);
  const [deleting, setDeleting] = useState<Channel | null>(null);

  const openCreate = () => {
    setEditing(null);
    setFormOpen(true);
  };

  const openEdit = (channel: Channel) => {
    setEditing(channel);
    setFormOpen(true);
  };

  const handleDelete = async () => {
    if (!deleting) return;
    try {
      await deleteMutation.mutateAsync(deleting.id);
      toast.success("删除成功", `渠道「${deleting.name}」已删除`);
      setDeleting(null);
    } catch (err) {
      toast.error("删除失败", (err as Error)?.message);
    }
  };

  const handleToggle = async (channel: Channel) => {
    const newStatus = channel.status === 1 ? 0 : 1;
    try {
      await toggleMutation.mutateAsync({ id: channel.id, status: newStatus });
      toast.success(
        newStatus === 1 ? "已启用" : "已禁用",
        `渠道「${channel.name}」${newStatus === 1 ? "已启用" : "已禁用"}`,
      );
    } catch (err) {
      toast.error("操作失败", (err as Error)?.message);
    }
  };

  const columns: Column<Channel>[] = [
    {
      key: "name",
      title: "渠道名称",
      render: (v, record) => (
        <div className="flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full flex-shrink-0 ${
              record.status === 1 ? "bg-success" : "bg-text-muted"
            }`}
            title={record.status === 1 ? "启用" : "禁用"}
          />
          <span className="font-medium text-text-primary">{String(v)}</span>
        </div>
      ),
    },
    {
      key: "type",
      title: "类型",
      width: "130px",
      render: (v) => {
        const info = getChannelType(String(v));
        return <Badge variant="info">{info?.label ?? String(v)}</Badge>;
      },
    },
    {
      key: "base_url",
      title: "Base URL",
      render: (v) => (
        <Tooltip content={<code className="text-xs mono break-all">{String(v)}</code>}>
          <code className="text-xs mono text-text-secondary truncate block max-w-[180px] cursor-default">
            {String(v)}
          </code>
        </Tooltip>
      ),
    },
    {
      key: "models",
      title: "模型",
      width: "160px",
      render: (v) => {
        const models = (v as string[]) ?? [];
        if (models.length === 0) return <span className="text-xs text-text-muted">-</span>;
        return (
          <div className="flex flex-wrap gap-1">
            {models.slice(0, 3).map((m) => (
              <span
                key={m}
                className="px-1.5 py-0.5 text-[10px] mono bg-bg-tertiary border border-border-primary rounded text-text-secondary"
              >
                {m}
              </span>
            ))}
            {models.length > 3 && (
              <span className="text-[10px] text-text-muted">+{models.length - 3}</span>
            )}
          </div>
        );
      },
    },
    {
      key: "status",
      title: "状态",
      width: "80px",
      render: (v) => (
        <Badge variant={Number(v) === 1 ? "success" : "neutral"}>
          {Number(v) === 1 ? "启用" : "禁用"}
        </Badge>
      ),
    },
    {
      key: "last_test_ok",
      title: "最近测试",
      width: "130px",
      render: (_v, record) => {
        if (record.last_test_at == null) {
          return <span className="text-xs text-text-muted">未测试</span>;
        }
        return (
          <div className="flex items-center gap-1.5">
            {record.last_test_ok === 1 ? (
              <span className="text-success">✓</span>
            ) : (
              <span className="text-danger">✗</span>
            )}
            <span className="text-xs text-text-muted">
              {formatTime(record.last_test_at)}
            </span>
          </div>
        );
      },
    },
    {
      key: "priority",
      title: "优先级",
      width: "80px",
      align: "right",
      render: (v) => <span className="text-xs tabular text-text-secondary">{String(v)}</span>,
    },
    {
      key: "actions",
      title: "操作",
      width: "220px",
      align: "right",
      render: (_v, record) => (
        <div className="flex items-center justify-end gap-1.5" onClick={(e) => e.stopPropagation()}>
          <ChannelTestButton channelId={record.id} channelName={record.name} />
          <Button
            variant="secondary"
            size="sm"
            onClick={() => openEdit(record)}
            title="编辑渠道"
          >
            <Pencil size={14} />
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => handleToggle(record)}
            title={record.status === 1 ? "禁用" : "启用"}
          >
            {record.status === 1 ? <PowerOff size={14} /> : <Power size={14} />}
          </Button>
          <Button
            variant="danger"
            size="sm"
            onClick={() => setDeleting(record)}
            title="删除渠道"
          >
            <Trash2 size={14} />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div>
      <PageHeader
        title="渠道管理"
        description="管理 LLM 上游提供方渠道"
        actions={
          <Button onClick={openCreate}>
            <Plus size={16} />
            添加渠道
          </Button>
        }
      />

      {/* 渠道列表 */}
      <Card noPadding>
        <Table
          columns={columns}
          data={channels ?? []}
          rowKey={(r) => r.id}
          loading={isLoading}
          emptyText="暂无渠道"
          emptyDescription="点击右上角「添加渠道」创建第一个渠道"
        />
      </Card>

      {/* 添加/编辑弹窗 */}
      <ChannelForm
        open={formOpen}
        channel={editing}
        onClose={() => {
          setFormOpen(false);
          setEditing(null);
        }}
      />

      {/* 删除确认 */}
      <ConfirmDialog
        open={!!deleting}
        title="删除渠道"
        description={`确定要删除渠道「${deleting?.name ?? ""}」吗？此操作不可撤销。`}
        confirmText="删除"
        danger
        loading={deleteMutation.isPending}
        onConfirm={handleDelete}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
};
