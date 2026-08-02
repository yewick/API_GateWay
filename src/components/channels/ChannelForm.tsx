import { useEffect, useState } from "react";
import type { Channel, CreateChannelInput, UpdateChannelInput } from "../../types";
import { CHANNEL_TYPES, getChannelType } from "../../lib/constants";
import { useCreateChannel, useUpdateChannel } from "../../hooks/useChannels";
import { Modal } from "../ui/Modal";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Button } from "../ui/Button";
import { toast } from "../../lib/toast";

interface ChannelFormProps {
  open: boolean;
  channel: Channel | null; // null = 创建模式
  onClose: () => void;
}

interface FormState {
  name: string;
  type: string;
  base_url: string;
  api_key: string;
  models: string;
  priority: string;
  weight: string;
}

const emptyForm: FormState = {
  name: "",
  type: "openai",
  base_url: "",
  api_key: "",
  models: "",
  priority: "0",
  weight: "1",
};

export function ChannelForm({ open, channel, onClose }: ChannelFormProps) {
  const isEdit = !!channel;
  const [form, setForm] = useState<FormState>(emptyForm);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const createMutation = useCreateChannel();
  const updateMutation = useUpdateChannel();

  // 打开时初始化表单
  useEffect(() => {
    if (!open) return;
    if (channel) {
      setForm({
        name: channel.name,
        type: channel.type,
        base_url: channel.base_url,
        api_key: channel.api_key,
        models: (channel.models ?? []).join(", "),
        priority: String(channel.priority ?? 0),
        weight: String(channel.weight ?? 1),
      });
    } else {
      const first = CHANNEL_TYPES[0];
      setForm({
        ...emptyForm,
        type: first.value,
        base_url: first.defaultBaseUrl,
      });
    }
    setErrors({});
  }, [open, channel]);

  const handleTypeChange = (type: string) => {
    const info = getChannelType(type);
    setForm((f) => ({
      ...f,
      type,
      base_url: info?.defaultBaseUrl ?? f.base_url,
      models: info?.defaultModels?.length
        ? info.defaultModels.join(", ")
        : f.models,
    }));
  };

  const set = (key: keyof FormState, value: string) =>
    setForm((f) => ({ ...f, [key]: value }));

  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!form.name.trim()) e.name = "请输入渠道名称";
    if (!form.type) e.type = "请选择渠道类型";
    if (!form.base_url.trim()) e.base_url = "请输入 Base URL";
    if (!form.api_key.trim()) e.api_key = "请输入 API Key";
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  const handleSubmit = async () => {
    if (!validate()) return;

    const models = form.models
      .split(",")
      .map((m) => m.trim())
      .filter(Boolean);

    if (isEdit && channel) {
      const input: UpdateChannelInput = {
        id: channel.id,
        name: form.name,
        type: form.type,
        base_url: form.base_url,
        api_key: form.api_key,
        models,
        priority: parseInt(form.priority, 10) || 0,
        weight: parseInt(form.weight, 10) || 1,
      };
      try {
        await updateMutation.mutateAsync(input);
        toast.success("更新成功", `渠道「${form.name}」已更新`);
        onClose();
      } catch (err) {
        toast.error("更新失败", (err as Error)?.message);
      }
    } else {
      const input: CreateChannelInput = {
        name: form.name,
        type: form.type,
        base_url: form.base_url,
        api_key: form.api_key,
        models,
        priority: parseInt(form.priority, 10) || 0,
        weight: parseInt(form.weight, 10) || 1,
      };
      try {
        await createMutation.mutateAsync(input);
        toast.success("创建成功", `渠道「${form.name}」已添加`);
        onClose();
      } catch (err) {
        toast.error("创建失败", (err as Error)?.message);
      }
    }
  };

  const loading = createMutation.isPending || updateMutation.isPending;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={isEdit ? "编辑渠道" : "添加渠道"}
      description={isEdit ? "修改渠道配置信息" : "创建新的 LLM 提供方渠道"}
      size="md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={loading}>
            取消
          </Button>
          <Button onClick={handleSubmit} loading={loading}>
            {isEdit ? "保存修改" : "创建渠道"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-4">
          <Input
            label="渠道名称"
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            placeholder="例如：OpenAI 主渠道"
            error={errors.name}
          />
          <Select
            label="渠道类型"
            value={form.type}
            onChange={(e) => handleTypeChange(e.target.value)}
            options={CHANNEL_TYPES.map((t) => ({
              value: t.value,
              label: `${t.label}（${t.category === "international" ? "国际" : t.category === "domestic" ? "国内" : t.category === "local" ? "本地" : "自定义"}）`,
            }))}
          />
        </div>

        <Input
          label="Base URL"
          value={form.base_url}
          onChange={(e) => set("base_url", e.target.value)}
          placeholder="https://api.openai.com/v1"
          hint="API 基础地址，选择渠道类型后会自动填充"
          error={errors.base_url}
        />

        <Input
          label="API Key"
          type="password"
          value={form.api_key}
          onChange={(e) => set("api_key", e.target.value)}
          placeholder="sk-..."
          hint="上游提供方的 API 密钥"
          error={errors.api_key}
        />

        <Input
          label="支持的模型"
          value={form.models}
          onChange={(e) => set("models", e.target.value)}
          placeholder="gpt-4o, gpt-4o-mini"
          hint="使用逗号分隔多个模型名称"
        />

        <div className="grid grid-cols-2 gap-4">
          <Input
            label="优先级"
            type="number"
            value={form.priority}
            onChange={(e) => set("priority", e.target.value)}
            hint="数值越高越优先被使用"
          />
          <Input
            label="权重"
            type="number"
            value={form.weight}
            onChange={(e) => set("weight", e.target.value)}
            hint="负载均衡权重，默认为 1"
          />
        </div>
      </div>
    </Modal>
  );
}
