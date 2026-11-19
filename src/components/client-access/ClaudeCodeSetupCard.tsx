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
import { Toggle } from "../ui/Toggle";
import { ConfigPreview } from "./ConfigPreview";
import { KeySourcePicker } from "./KeySourcePicker";
import { InfoRow, Notice } from "./SetupInfo";
import type {
  ApiKey,
  ClientSetupPreview,
  ClientSetupStatus,
  ClientTestResult,
  SetupModelOption,
} from "../../types";

interface ClaudeCodeSetupCardProps {
  status: ClientSetupStatus;
  models: SetupModelOption[];
  apiKeys: ApiKey[];
  keyMode: "new" | "existing";
  onKeyModeChange: (mode: "new" | "existing") => void;
  newKeyName: string;
  onNewKeyNameChange: (name: string) => void;
  existingKeyId: string;
  onExistingKeyIdChange: (id: string) => void;
  mainModel: string;
  onMainModelChange: (model: string) => void;
  mainCustomModel: string;
  onMainCustomModelChange: (model: string) => void;
  opusModel: string;
  onOpusModelChange: (model: string) => void;
  opusCustomModel: string;
  onOpusCustomModelChange: (model: string) => void;
  sonnetModel: string;
  onSonnetModelChange: (model: string) => void;
  sonnetCustomModel: string;
  onSonnetCustomModelChange: (model: string) => void;
  haikuModel: string;
  onHaikuModelChange: (model: string) => void;
  haikuCustomModel: string;
  onHaikuCustomModelChange: (model: string) => void;
  applyCli: boolean;
  onApplyCliChange: (value: boolean) => void;
  applyDesktop: boolean;
  onApplyDesktopChange: (value: boolean) => void;
  desktopDetected: boolean;
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

interface ModelFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  customValue: string;
  onCustomChange: (value: string) => void;
  models: SetupModelOption[];
  allowEmpty?: boolean;
  emptyLabel?: string;
  hint?: string;
}

function ModelField({
  label,
  value,
  onChange,
  customValue,
  onCustomChange,
  models,
  allowEmpty = false,
  emptyLabel = "跟随主模型",
  hint,
}: ModelFieldProps) {
  const options: { value: string; label: string }[] = [];
  if (allowEmpty) {
    options.push({ value: "", label: emptyLabel });
  }
  options.push(
    ...models.map((item) => ({
      value: item.id,
      label: item.owned_by ? `${item.id} · ${item.owned_by}` : item.id,
    })),
  );
  if (
    value &&
    value !== "__custom__" &&
    !models.some((item) => item.id === value)
  ) {
    options.unshift({ value, label: `${value}（当前配置）` });
  }
  options.push({ value: "__custom__", label: "自定义模型名…" });

  return (
    <div className="space-y-2">
      <Select
        label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        options={options}
        hint={hint}
      />
      {value === "__custom__" && (
        <Input
          value={customValue}
          onChange={(event) => onCustomChange(event.target.value)}
          placeholder="例如 deepseek-v4-pro"
        />
      )}
    </div>
  );
}

export function ClaudeCodeSetupCard({
  status,
  models,
  apiKeys,
  keyMode,
  onKeyModeChange,
  newKeyName,
  onNewKeyNameChange,
  existingKeyId,
  onExistingKeyIdChange,
  mainModel,
  onMainModelChange,
  mainCustomModel,
  onMainCustomModelChange,
  opusModel,
  onOpusModelChange,
  opusCustomModel,
  onOpusCustomModelChange,
  sonnetModel,
  onSonnetModelChange,
  sonnetCustomModel,
  onSonnetCustomModelChange,
  haikuModel,
  onHaikuModelChange,
  haikuCustomModel,
  onHaikuCustomModelChange,
  applyCli,
  onApplyCliChange,
  applyDesktop,
  onApplyDesktopChange,
  desktopDetected,
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
}: ClaudeCodeSetupCardProps) {
  const desktop = status.claude?.desktop;
  const desktopApplied = desktop?.applied ?? false;
  const anyApplied = status.applied || desktopApplied;

  const badge = status.config_parse_error
    ? { variant: "danger" as const, text: "配置无法解析" }
    : anyApplied
      ? { variant: "success" as const, text: "已接入" }
      : status.drift || desktop?.drift
        ? { variant: "warning" as const, text: "配置已变化" }
        : { variant: "neutral" as const, text: "未接入" };

  return (
    <Card
      title="Claude Code"
      description="CLI 与 Claude 桌面端共用模型路由，但配置位置不同：CLI 写 ~/.claude/settings.json，桌面端写第三方推理档案"
      headerRight={<Badge variant={badge.variant}>{badge.text}</Badge>}
    >
      <div className="space-y-5">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-3">
          <InfoRow label="配置目录" value={status.config_home} mono />
          <InfoRow label="配置文件" value={status.config_path} mono />
          <InfoRow label="网关地址" value={status.gateway_base_url} mono />
          <InfoRow label="当前主模型" value={status.current_model ?? "—"} />
          <InfoRow
            label="桌面端当前档案"
            value={
              desktop?.detected
                ? desktop.applied_profile_name ?? "未选择"
                : "未检测到"
            }
          />
          <InfoRow
            label="桌面端网关"
            value={desktop?.applied_base_url ?? "—"}
            mono
          />
        </div>

        {!status.installed && (
          <Notice tone="info">
            未检测到 Claude Code 配置目录，接入时会自动创建 {status.config_home}。
          </Notice>
        )}

        {status.config_parse_error && (
          <Notice tone="danger">
            现有配置无法解析：{status.config_parse_error}
            。为避免破坏文件，YeAPI 不会写入；请先手动修复。
          </Notice>
        )}

        {(status.drift || desktop?.drift) && (
          <Notice tone="warning">
            接入配置在写入后被修改过。可以重新接入覆盖，或断开接入恢复。
          </Notice>
        )}

        {status.applied && !status.base_url_matches && (
          <Notice tone="warning">
            网关地址已变化（当前为 {status.gateway_base_url}
            ），请重新接入以更新 Claude Code 配置。
          </Notice>
        )}

        {desktopApplied && desktop && !desktop.base_url_matches && (
          <Notice tone="warning">
            桌面端档案指向 {desktop.applied_base_url ?? "未知地址"}
            ，与当前网关不一致，请重新接入。
          </Notice>
        )}

        {anyApplied && !status.applied_api_key_valid && (
          <Notice tone="danger">
            接入使用的密钥已被删除、禁用或过期，请重新接入。
          </Notice>
        )}

        {status.claude?.anthropic_api_key_present && (
          <Notice tone="warning">
            检测到当前环境已有 ANTHROPIC_API_KEY，可能与 YeAPI 写入的
            ANTHROPIC_AUTH_TOKEN 冲突，建议移除后者以外的旧配置。
          </Notice>
        )}

        {status.claude?.top_level_model && (
          <Notice tone="warning">
            settings.json 顶层 model 为 {status.claude.top_level_model}
            ，可能覆盖 ANTHROPIC_MODEL；如需 YeAPI 默认模型生效，请移除该字段。
          </Notice>
        )}

        <Notice tone="info">
          接入后 Claude Code 的请求与用量会计入 YeAPI，不再走 Anthropic
          账号。若同时使用 cc-switch 管理桌面端，两者可能互相覆盖，建议只保留一个。
        </Notice>

        <div className="space-y-2 rounded-lg border border-border-primary p-3">
          <Toggle
            checked={applyCli}
            onChange={onApplyCliChange}
            label="Claude Code CLI / 终端"
            description="写入 ~/.claude/settings.json 的 env"
          />
          <Toggle
            checked={applyDesktop && desktopDetected}
            onChange={onApplyDesktopChange}
            disabled={!desktopDetected}
            label="Claude 桌面端"
            description={
              desktopDetected
                ? `写入 ${desktop?.config_dir ?? "桌面端"} 的第三方推理档案`
                : "未检测到桌面端第三方推理配置（见下方说明）"
            }
          />
        </div>

        {!desktopDetected && (
          <Notice tone="info">
            如需接入桌面端，请先在 Claude 桌面端打开 Help → Troubleshooting →
            启用 Developer Mode，再进入 Developer → Configure Third-Party
            Inference 保存一次配置，然后回到本页刷新。
          </Notice>
        )}

        <KeySourcePicker
          mode={keyMode}
          onModeChange={onKeyModeChange}
          name={newKeyName}
          onNameChange={onNewKeyNameChange}
          keys={apiKeys}
          keyId={existingKeyId}
          onKeyIdChange={onExistingKeyIdChange}
          namePlaceholder="Claude Code"
          nameHint="接入时自动创建；明文只写入 Claude Code 配置，不在界面回显。"
        />

        <div className="space-y-3">
          <ModelField
            label="默认模型（ANTHROPIC_MODEL）"
            value={mainModel}
            onChange={onMainModelChange}
            customValue={mainCustomModel}
            onCustomChange={onMainCustomModelChange}
            models={models}
            hint={
              models.length === 0
                ? "未找到启用渠道的模型，请先在「渠道」页添加并启用渠道，或手动输入。"
                : "模型来自已启用渠道；也可手动输入模型名。"
            }
          />
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            <ModelField
              label="Opus 映射"
              value={opusModel}
              onChange={onOpusModelChange}
              customValue={opusCustomModel}
              onCustomChange={onOpusCustomModelChange}
              models={models}
              allowEmpty
            />
            <ModelField
              label="Sonnet 映射"
              value={sonnetModel}
              onChange={onSonnetModelChange}
              customValue={sonnetCustomModel}
              onCustomChange={onSonnetCustomModelChange}
              models={models}
              allowEmpty
            />
            <ModelField
              label="Haiku 映射"
              value={haikuModel}
              onChange={onHaikuModelChange}
              customValue={haikuCustomModel}
              onCustomChange={onHaikuCustomModelChange}
              models={models}
              allowEmpty
            />
          </div>
          <p className="text-xs text-text-muted">
            别名留空时跟随默认模型；Haiku 映射也用于会话标题等后台任务。
          </p>
        </div>

        {preview?.warning && <Notice tone="warning">{preview.warning}</Notice>}

        <p className="text-xs text-text-muted">
          以下仅显示将写入 / 覆盖的键；settings.json
          中的其他设置（enabledPlugins、theme 等）保持不变。
        </p>

        {applyCli && (
          <ConfigPreview
            path={preview?.config_path ?? status.config_path}
            snippet={preview?.snippet ?? ""}
            loading={previewLoading}
          />
        )}

        {applyDesktop && preview?.desktop && (
          <div className="space-y-2">
            {preview.desktop.warning && (
              <Notice tone="warning">{preview.desktop.warning}</Notice>
            )}
            <p className="text-xs text-text-muted">
              桌面端将新建 / 更新名为 YeAPI 的第三方推理档案，并切换过去；断开时恢复原档案。
            </p>
            <ConfigPreview
              path={preview.desktop.config_path}
              snippet={preview.desktop.snippet}
              loading={previewLoading}
            />
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={onApply} loading={applying}>
            <RefreshCw size={15} />
            {anyApplied ? "重新接入" : "接入 Claude Code"}
          </Button>
          {anyApplied && (
            <Button variant="secondary" onClick={onTest} loading={testing}>
              <Wifi size={15} />
              测试连接
            </Button>
          )}
          {(anyApplied || status.drift || desktop?.drift) && (
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
            配置已写入。请完全退出 Claude Code CLI 与 Claude 桌面端（Cmd+Q
            退出进程，关闭窗口不算）后重开：CLI 请重开终端，桌面端在 Code
            标签页新建会话后生效。
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
