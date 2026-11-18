import type { ReactNode } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FolderOpen,
  RefreshCw,
  Unplug,
  Wifi,
} from "lucide-react";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { ConfigPreview } from "./ConfigPreview";
import { KeySourcePicker } from "./KeySourcePicker";
import type {
  ApiKey,
  ClientSetupPreview,
  ClientSetupStatus,
  ClientTestResult,
  SetupModelOption,
} from "../../types";

interface CodexSetupCardProps {
  status: ClientSetupStatus;
  models: SetupModelOption[];
  apiKeys: ApiKey[];
  keyMode: "new" | "existing";
  onKeyModeChange: (mode: "new" | "existing") => void;
  newKeyName: string;
  onNewKeyNameChange: (name: string) => void;
  existingKeyId: string;
  onExistingKeyIdChange: (id: string) => void;
  model: string;
  onModelChange: (model: string) => void;
  customModel: string;
  onCustomModelChange: (model: string) => void;
  preview?: ClientSetupPreview;
  previewLoading: boolean;
  applying: boolean;
  testing: boolean;
  onApply: () => void;
  onDisconnect: () => void;
  onTest: () => void;
  onOpenDir: () => void;
  testResult: ClientTestResult | null;
  restartRequired: boolean;
  disconnectedWarnings: string[];
}

function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] text-text-muted">{label}</div>
      <div
        className={`text-xs text-text-primary truncate ${mono ? "mono" : ""}`}
        title={value}
      >
        {value}
      </div>
    </div>
  );
}

function Notice({
  tone,
  children,
}: {
  tone: "info" | "warning" | "danger";
  children: ReactNode;
}) {
  const classes = {
    info: "bg-info/10 border-info/30 text-info",
    warning: "bg-warning/10 border-warning/30 text-warning",
    danger: "bg-danger/10 border-danger/30 text-danger",
  }[tone];
  return (
    <div className={`rounded-lg border px-3 py-2 text-xs leading-relaxed ${classes}`}>
      {children}
    </div>
  );
}

export function CodexSetupCard({
  status,
  models,
  apiKeys,
  keyMode,
  onKeyModeChange,
  newKeyName,
  onNewKeyNameChange,
  existingKeyId,
  onExistingKeyIdChange,
  model,
  onModelChange,
  customModel,
  onCustomModelChange,
  preview,
  previewLoading,
  applying,
  testing,
  onApply,
  onDisconnect,
  onTest,
  onOpenDir,
  testResult,
  restartRequired,
  disconnectedWarnings,
}: CodexSetupCardProps) {
  const badge = status.config_parse_error
    ? { variant: "danger" as const, text: "配置无法解析" }
    : status.applied
      ? { variant: "success" as const, text: "已接入" }
      : status.drift
        ? { variant: "warning" as const, text: "配置已变化" }
        : { variant: "neutral" as const, text: "未接入" };

  const modelOptions = models.map((item) => ({
    value: item.id,
    label: item.owned_by ? `${item.id} · ${item.owned_by}` : item.id,
  }));
  if (
    model &&
    model !== "__custom__" &&
    !models.some((item) => item.id === model)
  ) {
    modelOptions.unshift({ value: model, label: `${model}（当前配置）` });
  }
  modelOptions.push({ value: "__custom__", label: "自定义模型名…" });

  return (
    <Card
      title="Codex"
      description="CLI 与 Codex 桌面 App 共用 ~/.codex/config.toml"
      headerRight={<Badge variant={badge.variant}>{badge.text}</Badge>}
    >
      <div className="space-y-5">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-3">
          <InfoRow label="配置目录" value={status.codex_home} mono />
          <InfoRow label="网关地址" value={status.gateway_base_url} mono />
          <InfoRow
            label="当前 provider"
            value={status.current_model_provider ?? "—"}
          />
          <InfoRow label="当前模型" value={status.current_model ?? "—"} />
        </div>

        {!status.installed && (
          <Notice tone="info">
            未检测到 Codex 配置目录，接入时会自动创建 {status.codex_home}。
          </Notice>
        )}

        {status.config_parse_error && (
          <Notice tone="danger">
            现有配置无法解析：{status.config_parse_error}
            。为避免破坏文件，YeAPI 不会写入；请先手动修复。
          </Notice>
        )}

        {status.drift && (
          <Notice tone="warning">
            配置文件在接入后被修改过（provider、模型或密钥不再是 YeAPI
            写入的值）。可以重新接入覆盖，或断开接入。
          </Notice>
        )}

        {status.applied && !status.base_url_matches && (
          <Notice tone="warning">
            网关地址已变化（当前为 {status.gateway_base_url}
            ），请重新接入以更新 Codex 配置。
          </Notice>
        )}

        {status.applied && !status.applied_api_key_valid && (
          <Notice tone="danger">
            接入使用的密钥已被删除、禁用或过期，请重新接入。
          </Notice>
        )}

        {status.auth_logged_in && (
          <Notice tone="info">
            检测到本机存在 ChatGPT 登录，将写入 requires_openai_auth =
            true；接入不会修改 auth.json。
          </Notice>
        )}

        {status.applied && status.applied_api_key_hint && (
          <div className="text-xs text-text-muted">
            当前密钥：<span className="mono text-text-secondary">{status.applied_api_key_hint}</span>
            {status.applied_at && (
              <span className="ml-3">接入时间：{status.applied_at}</span>
            )}
          </div>
        )}

        <KeySourcePicker
          mode={keyMode}
          onModeChange={onKeyModeChange}
          name={newKeyName}
          onNameChange={onNewKeyNameChange}
          keys={apiKeys}
          keyId={existingKeyId}
          onKeyIdChange={onExistingKeyIdChange}
        />

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Select
            label="默认模型"
            value={model}
            onChange={(event) => onModelChange(event.target.value)}
            options={modelOptions}
            placeholder="请选择模型"
            hint={
              models.length === 0
                ? "未找到启用渠道的模型，请先在「渠道」页添加并启用渠道，或手动输入。"
                : "模型来自已启用渠道；也可手动输入模型名。"
            }
          />
          {model === "__custom__" && (
            <Input
              label="自定义模型名"
              value={customModel}
              onChange={(event) => onCustomModelChange(event.target.value)}
              placeholder="例如 deepseek-v4-pro"
            />
          )}
        </div>

        <ConfigPreview
          path={preview?.config_path ?? status.config_path}
          snippet={preview?.snippet ?? ""}
          loading={previewLoading}
        />

        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={onApply} loading={applying}>
            <RefreshCw size={15} />
            {status.applied || status.yeapi_provider_present
              ? "重新接入"
              : "接入 Codex"}
          </Button>
          {status.applied && (
            <Button variant="secondary" onClick={onTest} loading={testing}>
              <Wifi size={15} />
              测试连接
            </Button>
          )}
          {(status.applied || status.drift) && (
            <Button variant="danger" onClick={onDisconnect}>
              <Unplug size={15} />
              断开接入
            </Button>
          )}
          <Button variant="ghost" onClick={onOpenDir}>
            <FolderOpen size={15} />
            打开配置目录
          </Button>
        </div>

        {testResult && (
          <div
            className={`rounded-lg border px-3 py-2 text-xs ${
              testResult.ok
                ? "bg-success/10 border-success/30 text-success"
                : "bg-danger/10 border-danger/30 text-danger"
            }`}
          >
            <div className="flex items-center gap-1.5 font-medium">
              {testResult.ok ? (
                <CheckCircle2 size={13} />
              ) : (
                <AlertTriangle size={13} />
              )}
              {testResult.ok ? "连接成功" : "连接失败"}（HTTP {testResult.status}，
              {testResult.latency_ms} ms）
            </div>
            <div className="mt-1 text-text-secondary break-all">
              {testResult.message}
            </div>
          </div>
        )}

        {restartRequired && (
          <Notice tone="warning">
            配置已写入。请完全退出 Codex 后重新打开：桌面 App 使用 Cmd+Q
            退出进程（关闭窗口不算），CLI 请结束正在运行的 codex 进程并重开终端。
          </Notice>
        )}

        {disconnectedWarnings.map((warning, index) => (
          <Notice key={`${index}-${warning}`} tone="warning">
            {warning}
          </Notice>
        ))}
      </div>
    </Card>
  );
}
