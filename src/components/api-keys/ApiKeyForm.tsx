import { useEffect, useState } from "react";
import { Copy, Check } from "lucide-react";
import type { CreateApiKeyInput } from "../../types";
import { useCreateApiKey } from "../../hooks/useApiKeys";
import { useChannels } from "../../hooks/useChannels";
import { Modal } from "../ui/Modal";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { toast } from "../../lib/toast";

interface ApiKeyFormProps {
  open: boolean;
  onClose: () => void;
}

interface FormState {
  name: string;
  allowed_models: string;
  allowed_channels: string[];
  quota_limit: string;
  expires_at: string;
}

const emptyForm: FormState = {
  name: "",
  allowed_models: "",
  allowed_channels: [],
  quota_limit: "-1",
  expires_at: "",
};

export function ApiKeyForm({ open, onClose }: ApiKeyFormProps) {
  const [form, setForm] = useState<FormState>(emptyForm);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const createMutation = useCreateApiKey();
  const { data: channels } = useChannels();

  // 打开时重置表单
  useEffect(() => {
    if (open) {
      setForm(emptyForm);
      setErrors({});
      setCreatedKey(null);
      setCopied(false);
    }
  }, [open]);

  const set = (key: keyof FormState, value: string | string[]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const toggleChannel = (id: string) => {
    setForm((f) => ({
      ...f,
      allowed_channels: f.allowed_channels.includes(id)
        ? f.allowed_channels.filter((c) => c !== id)
        : [...f.allowed_channels, id],
    }));
  };

  const handleCreate = async () => {
    if (!form.name.trim()) {
      setErrors({ name: "请输入密钥名称" });
      return;
    }
    setErrors({});

    const input: CreateApiKeyInput = {
      name: form.name,
      allowed_models: form.allowed_models
        .split(",")
        .map((m) => m.trim())
        .filter(Boolean),
      allowed_channels: form.allowed_channels,
      quota_limit: parseInt(form.quota_limit, 10) || -1,
      expires_at: form.expires_at ? new Date(form.expires_at).toISOString() : null,
    };

    try {
      const result = await createMutation.mutateAsync(input);
      setCreatedKey(result.key);
      toast.success("创建成功", "新密钥已生成，请立即复制保存");
    } catch (err) {
      toast.error("创建失败", (err as Error)?.message);
    }
  };

  const copyKey = async () => {
    if (!createdKey) return;
    try {
      await navigator.clipboard.writeText(createdKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      toast.success("已复制", "密钥已复制到剪贴板");
    } catch {
      toast.error("复制失败", "无法访问剪贴板");
    }
  };

  const handleClose = () => {
    // 未保存密钥时关闭给出提示
    if (createdKey) {
      toast.warning("请妥善保存密钥", "关闭后将无法再次查看完整密钥");
    }
    onClose();
  };

  return (
    <Modal
      open={open}
      onClose={handleClose}
      title="创建 API 密钥"
      description="密钥用于通过网关访问 LLM 服务"
      size="md"
      footer={
        createdKey ? (
          <>
            <Button variant="ghost" onClick={handleClose}>
              完成
            </Button>
            <Button onClick={copyKey}>
              {copied ? <Check size={16} /> : <Copy size={16} />}
              {copied ? "已复制" : "复制密钥"}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={onClose}>
              取消
            </Button>
            <Button onClick={handleCreate} loading={createMutation.isPending}>
              创建密钥
            </Button>
          </>
        )
      }
    >
      {createdKey ? (
        <div className="space-y-4">
          <div className="p-4 bg-warning/10 border border-warning/30 rounded-lg">
            <p className="text-sm text-warning font-medium mb-2">
              ⚠️ 密钥仅显示一次，请立即保存！
            </p>
            <p className="text-xs text-text-secondary">
              出于安全考虑，关闭此弹窗后将无法再次查看完整密钥内容。
            </p>
          </div>
          <div className="flex items-center gap-2">
            <code className="flex-1 px-3 py-2.5 bg-bg-tertiary border border-border-primary rounded-lg text-sm mono break-all text-text-primary">
              {createdKey}
            </code>
            <Button variant="secondary" size="sm" onClick={copyKey}>
              {copied ? <Check size={14} /> : <Copy size={14} />}
            </Button>
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          <Input
            label="密钥名称"
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            placeholder="例如：默认密钥"
            error={errors.name}
          />

          <Input
            label="允许的模型"
            value={form.allowed_models}
            onChange={(e) => set("allowed_models", e.target.value)}
            placeholder="gpt-4o, deepseek-v4-flash"
            hint="留空则允许所有模型，逗号分隔"
          />

          <div>
            <label className="block mb-1.5 text-sm font-medium text-text-secondary">
              允许的渠道
            </label>
            <div className="flex flex-wrap gap-2">
              {(channels ?? []).map((c) => {
                const active = form.allowed_channels.includes(c.id);
                return (
                  <button
                    key={c.id}
                    type="button"
                    onClick={() => toggleChannel(c.id)}
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium border transition-colors ${
                      active
                        ? "bg-accent/15 text-accent border-accent/40"
                        : "bg-bg-tertiary text-text-secondary border-border-primary hover:bg-bg-hover"
                    }`}
                  >
                    {c.name}
                  </button>
                );
              })}
              {(channels ?? []).length === 0 && (
                <p className="text-xs text-text-muted">暂无渠道，可稍后在渠道页添加</p>
              )}
            </div>
            <p className="mt-1 text-xs text-text-muted">
              留空表示允许使用所有渠道
            </p>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <Input
              label="配额限制（Token）"
              type="number"
              value={form.quota_limit}
              onChange={(e) => set("quota_limit", e.target.value)}
              hint="-1 表示不限制"
            />
            <Input
              label="过期时间"
              type="date"
              value={form.expires_at}
              onChange={(e) => set("expires_at", e.target.value)}
              hint="留空表示永不过期"
            />
          </div>
        </div>
      )}
    </Modal>
  );
}
