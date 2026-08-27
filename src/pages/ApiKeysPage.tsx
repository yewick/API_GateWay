import { useState } from "react";
import { Plus, Trash2, Copy, Check, Power, PowerOff } from "lucide-react";
import type { ApiKey } from "../types";
import {
  useApiKeys,
  useDeleteApiKey,
  useUpdateApiKey,
} from "../hooks/useApiKeys";
import { PageHeader } from "../components/ui/PageHeader";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Table, type Column } from "../components/ui/Table";
import { Card } from "../components/ui/Card";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { ApiKeyForm } from "../components/api-keys/ApiKeyForm";
import { QuotaRing } from "../components/api-keys/QuotaRing";
import { formatTime } from "../lib/constants";
import { toast } from "../lib/toast";

// 脱敏密钥展示
const maskKey = (key: string): string => {
  if (key.length <= 14) return "sk-yeapi-••••";
  return `${key.slice(0, 12)}••••••••${key.slice(-4)}`;
};

export const ApiKeysPage = () => {
  const { data: apiKeys, isLoading } = useApiKeys();
  const deleteMutation = useDeleteApiKey();
  const updateMutation = useUpdateApiKey();

  const [formOpen, setFormOpen] = useState(false);
  const [deleting, setDeleting] = useState<ApiKey | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const copyKey = async (key: string) => {
    try {
      await navigator.clipboard.writeText(key);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
      toast.success("已复制", "密钥已复制到剪贴板");
    } catch {
      toast.error("复制失败", "无法访问剪贴板");
    }
  };

  const handleToggle = async (apiKey: ApiKey) => {
    const newStatus = apiKey.status === 1 ? 0 : 1;
    try {
      await updateMutation.mutateAsync({ id: apiKey.id, status: newStatus });
      toast.success(
        newStatus === 1 ? "已启用" : "已禁用",
        `密钥「${apiKey.name}」${newStatus === 1 ? "已启用" : "已禁用"}`,
      );
    } catch (err) {
      toast.error("操作失败", (err as Error)?.message);
    }
  };

  const handleDelete = async () => {
    if (!deleting) return;
    try {
      await deleteMutation.mutateAsync(deleting.id);
      toast.success("删除成功", `密钥「${deleting.name}」已删除`);
      setDeleting(null);
    } catch (err) {
      toast.error("删除失败", (err as Error)?.message);
    }
  };

  const columns: Column<ApiKey>[] = [
    {
      key: "name",
      title: "名称",
      render: (v, record) => (
        <div className="flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full flex-shrink-0 ${
              record.status === 1 ? "bg-success" : "bg-text-muted"
            }`}
          />
          <span className="font-medium text-text-primary">{String(v)}</span>
        </div>
      ),
    },
    {
      key: "key",
      title: "密钥",
      render: (v) => {
        const key = String(v);
        return (
          <div className="flex items-center gap-1.5">
            <code className="text-xs mono text-text-secondary">{maskKey(key)}</code>
            <button
              onClick={(e) => {
                e.stopPropagation();
                copyKey(key);
              }}
              className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
              title="复制完整密钥"
            >
              {copiedKey === key ? <Check size={13} /> : <Copy size={13} />}
            </button>
          </div>
        );
      },
    },
    {
      key: "quota_used",
      title: "配额使用",
      width: "220px",
      render: (_v, record) => {
        if (record.quota_limit === -1) {
          return (
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-full border-2 border-dashed border-border-primary flex items-center justify-center text-text-muted text-xs flex-shrink-0">
                ∞
              </div>
              <span className="text-xs text-text-muted tabular">不限</span>
            </div>
          );
        }
        const pct = Math.min(100, (record.quota_used / record.quota_limit) * 100);
        return (
          <div className="flex items-center gap-3">
            <QuotaRing used={record.quota_used} limit={record.quota_limit} />
            <div>
              <div className="text-xs text-text-secondary tabular">
                {(record.quota_used / 1000).toFixed(1)}k / {(record.quota_limit / 1000).toFixed(1)}k
              </div>
              <div className="text-[10px] text-text-muted tabular">
                {pct.toFixed(0)}% 已使用
              </div>
            </div>
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
      key: "expires_at",
      title: "过期时间",
      width: "130px",
      render: (v) => {
        if (!v) return <span className="text-xs text-text-muted">永不过期</span>;
        return <span className="text-xs text-text-secondary">{formatTime(String(v))}</span>;
      },
    },
    {
      key: "created_at",
      title: "创建时间",
      width: "130px",
      render: (v) => (
        <span className="text-xs text-text-muted">{formatTime(String(v))}</span>
      ),
    },
    {
      key: "actions",
      title: "操作",
      width: "130px",
      align: "right",
      render: (_v, record) => (
        <div
          className="flex items-center justify-end gap-1.5"
          onClick={(e) => e.stopPropagation()}
        >
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
            title="删除密钥"
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
        title="密钥管理"
        description="管理用于访问网关的 API 密钥"
        actions={
          <Button onClick={() => setFormOpen(true)}>
            <Plus size={16} />
            创建密钥
          </Button>
        }
      />

      <Card noPadding>
        <Table
          columns={columns}
          data={apiKeys ?? []}
          rowKey={(r) => r.id}
          loading={isLoading}
          emptyText="暂无密钥"
          emptyDescription="点击右上角「创建密钥」生成新的 sk-yeapi-* 格式密钥"
        />
      </Card>

      {/* 创建密钥弹窗 */}
      <ApiKeyForm open={formOpen} onClose={() => setFormOpen(false)} />

      {/* 删除确认 */}
      <ConfirmDialog
        open={!!deleting}
        title="删除密钥"
        description={`确定要删除密钥「${deleting?.name ?? ""}」吗？使用该密钥的请求将立即失效。`}
        confirmText="删除"
        danger
        loading={deleteMutation.isPending}
        onConfirm={handleDelete}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
};
