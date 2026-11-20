import { useEffect, useMemo, useRef, useState } from "react";
import { FolderOpen, Wifi } from "lucide-react";
import { Button } from "../ui/Button";
import { Modal } from "../ui/Modal";
import { clientSetupApi } from "../../lib/api";
import {
  useApplyClientSetup,
  useClientSetupPreview,
  useTestClientSetup,
} from "../../hooks/useClientSetup";
import { toast } from "../../lib/toast";
import type {
  ApiKey,
  ApplyClientInput,
  ClientKind,
  ClientSetupStatus,
  ClientTestResult,
  SetupModelOption,
} from "../../types";
import type { TargetId } from "../../lib/clientAccess";
import { ConfigPreview } from "./ConfigPreview";
import { KeySourcePicker } from "./KeySourcePicker";
import { Notice } from "./SetupInfo";
import {
  ModelField,
  aliasValue,
} from "./ClientSetupFields";

interface ClientSetupDialogProps {
  target: TargetId | null;
  onClose: () => void;
  codexStatus?: ClientSetupStatus;
  claudeStatus?: ClientSetupStatus;
  models: SetupModelOption[];
  apiKeys: ApiKey[];
}

const targetMeta: Record<
  TargetId,
  { client: ClientKind; title: string; description: string; keyName: string }
> = {
  codex: {
    client: "codex",
    title: "接入 Codex CLI",
    description: "写入 ~/.codex/config.toml 的 yeapi provider，使用 Responses 协议。",
    keyName: "Codex",
  },
  "claude-cli": {
    client: "claude",
    title: "接入 Claude Code CLI",
    description: "写入 ~/.claude/settings.json 的 ANTHROPIC_* 环境变量。",
    keyName: "Claude Code",
  },
  "claude-desktop": {
    client: "claude",
    title: "接入 Claude Desktop",
    description: "创建并切换 YeAPI 第三方推理档案，按槽位映射到真实模型。",
    keyName: "Claude Desktop",
  },
};

export function ClientSetupDialog({
  target,
  onClose,
  codexStatus,
  claudeStatus,
  models,
  apiKeys,
}: ClientSetupDialogProps) {
  const meta = target ? targetMeta[target] : null;
  const client: ClientKind = meta?.client ?? "codex";
  const status = client === "codex" ? codexStatus : claudeStatus;

  const [keyMode, setKeyMode] = useState<"new" | "existing">("new");
  const [newKeyName, setNewKeyName] = useState("Codex");
  const [existingKeyId, setExistingKeyId] = useState("");
  const [model, setModel] = useState("");
  const [customModel, setCustomModel] = useState("");
  const [opus, setOpus] = useState("");
  const [opusCustom, setOpusCustom] = useState("");
  const [sonnet, setSonnet] = useState("");
  const [sonnetCustom, setSonnetCustom] = useState("");
  const [haiku, setHaiku] = useState("");
  const [haikuCustom, setHaikuCustom] = useState("");
  const [fable, setFable] = useState("");
  const [fableCustom, setFableCustom] = useState("");
  const [testResult, setTestResult] = useState<ClientTestResult | null>(null);

  const applyMutation = useApplyClientSetup();
  const testMutation = useTestClientSetup();

  const usableKeys = useMemo(
    () =>
      apiKeys.filter(
        (key) =>
          key.status === 1 &&
          (!key.expires_at || new Date(key.expires_at).getTime() > Date.now()),
      ),
    [apiKeys],
  );

  // 选择已有密钥时，模型选项收紧到该密钥允许的模型；新建密钥默认不受限。
  const allowedIdSet = useMemo(() => {
    if (keyMode !== "existing") return null;
    const key = usableKeys.find((item) => item.id === existingKeyId);
    if (!key || key.allowed_models.length === 0) return null;
    return new Set(key.allowed_models);
  }, [keyMode, existingKeyId, usableKeys]);

  const modelChoices = useMemo(
    () =>
      allowedIdSet
        ? models.filter((item) => allowedIdSet.has(item.id))
        : models,
    [models, allowedIdSet],
  );

  const desktopDetected = Boolean(claudeStatus?.claude?.desktop.detected);
  const appliedHere = !target
    ? false
    : target === "codex"
      ? Boolean(codexStatus?.applied)
      : target === "claude-cli"
        ? Boolean(claudeStatus?.claude?.cli_applied)
        : Boolean(claudeStatus?.claude?.desktop.applied);

  const initRef = useRef("");
  useEffect(() => {
    if (!target || !meta) {
      initRef.current = "";
      return;
    }
    const signature = `${target}:${status?.applied_at ?? "none"}:${models.length}:${apiKeys.length}`;
    if (initRef.current === signature) return;
    initRef.current = signature;

    const usable = apiKeys.find(
      (key) =>
        key.id === status?.applied_api_key_id &&
        key.status === 1 &&
        (!key.expires_at || new Date(key.expires_at).getTime() > Date.now()),
    );
    const useExisting = Boolean(status?.applied_at && usable);
    setKeyMode(useExisting ? "existing" : "new");
    setExistingKeyId(useExisting && usable ? usable.id : "");
    setNewKeyName(meta.keyName);

    // 默认只选「当前可用（且密钥允许）」的模型；旧配置里已下线的模型
    // 不再以「自定义模型」预填，避免一打开就带着一个不可用的模型。
    const allowedForInit =
      useExisting && usable && usable.allowed_models.length > 0
        ? new Set(usable.allowed_models)
        : null;
    const candidates = allowedForInit
      ? models.filter((item) => allowedForInit.has(item.id))
      : models;
    const pickAvailable = (value: string | null | undefined) =>
      value && candidates.some((item) => item.id === value) ? value : "";
    const fallback = candidates[0]?.id ?? "";

    setModel(pickAvailable(status?.current_model) || fallback);
    setCustomModel("");

    const env = status?.claude?.env_models;
    setOpus(pickAvailable(env?.opus));
    setOpusCustom("");
    setSonnet(pickAvailable(env?.sonnet));
    setSonnetCustom("");
    setHaiku(pickAvailable(env?.haiku));
    setHaikuCustom("");
    setFable(pickAvailable(env?.fable));
    setFableCustom("");
    setTestResult(null);
  }, [target, meta, status?.applied_at, status?.current_model, status?.claude?.env_models, models, apiKeys]);

  // 密钥允许模型变化时，把已选但不再可用的模型收敛到允许范围内。
  useEffect(() => {
    if (!allowedIdSet) return;
    const first = modelChoices[0]?.id ?? "";
    const clamp = (value: string) =>
      value && (value === "__custom__" || !allowedIdSet.has(value)) ? "" : value;
    setModel((prev) => clamp(prev) || first);
    setOpus((prev) => clamp(prev));
    setSonnet((prev) => clamp(prev));
    setHaiku((prev) => clamp(prev));
    setFable((prev) => clamp(prev));
  }, [allowedIdSet, modelChoices]);

  const selectedModel = model === "__custom__" ? customModel.trim() : model;
  const claudeOptions = useMemo(
    () => ({
      opus_model: aliasValue(opus, opusCustom),
      sonnet_model: aliasValue(sonnet, sonnetCustom),
      haiku_model: aliasValue(haiku, haikuCustom),
      fable_model: aliasValue(fable, fableCustom),
    }),
    [opus, opusCustom, sonnet, sonnetCustom, haiku, haikuCustom, fable, fableCustom],
  );

  const selectedAliasModels =
    client === "claude"
      ? [
          claudeOptions.opus_model,
          claudeOptions.sonnet_model,
          claudeOptions.haiku_model,
          claudeOptions.fable_model,
        ]
      : [];
  const unavailableModel = [selectedModel, ...selectedAliasModels]
    .filter((value) => Boolean(value))
    .find((value) => !modelChoices.some((item) => item.id === value));

  const previewInput = useMemo<ApplyClientInput | null>(() => {
    if (!selectedModel) return null;
    if (client === "codex") {
      return {
        client: "codex",
        key: { mode: "new", name: "preview" },
        model: selectedModel,
      };
    }
    return {
      client: "claude",
      key: { mode: "new", name: "preview" },
      model: selectedModel,
      claude: {
        ...claudeOptions,
        apply_cli: target === "claude-cli",
        apply_desktop: target === "claude-desktop",
      },
    };
  }, [client, target, selectedModel, claudeOptions]);

  const { data: preview, isFetching: previewLoading } =
    useClientSetupPreview(previewInput);

  if (!target || !meta) return null;

  const handleTest = async () => {
    try {
      setTestResult(await testMutation.mutateAsync(client));
    } catch (error) {
      toast.error("测试失败", (error as Error).message);
    }
  };

  const handleOpenDir = async () => {
    try {
      await clientSetupApi.openDir(client);
    } catch (error) {
      toast.error("打开失败", (error as Error).message);
    }
  };

  const handleApply = async () => {
    if (!selectedModel) {
      toast.error("缺少模型", "请选择或输入默认模型");
      return;
    }
    if (unavailableModel) {
      toast.error(
        "模型不可用",
        `模型 \`${unavailableModel}\` 没有匹配的启用渠道，请重新选择`,
      );
      return;
    }
    if (keyMode === "existing" && !existingKeyId) {
      toast.error("缺少密钥", "请选择一个已有密钥");
      return;
    }
    if (target === "claude-desktop" && !desktopDetected) {
      toast.error(
        "未检测到桌面端配置",
        "请先在 Claude 桌面端开启 Developer Mode 并配置一次第三方推理",
      );
      return;
    }

    const input: ApplyClientInput = {
      client,
      key:
        keyMode === "existing"
          ? { mode: "existing", id: existingKeyId }
          : {
              mode: "new",
              name: newKeyName.trim() || meta.keyName,
              allowed_models: [],
              quota_limit: -1,
            },
      model: selectedModel,
      claude:
        client === "claude"
          ? {
              ...claudeOptions,
              apply_cli: target === "claude-cli",
              apply_desktop: target === "claude-desktop",
            }
          : undefined,
    };

    try {
      const result = await applyMutation.mutateAsync(input);
      toast.success(
        appliedHere ? "接入已更新" : "接入完成",
        result.restart_required
          ? "请完全退出并重开客户端使配置生效"
          : undefined,
      );
      onClose();
    } catch (error) {
      toast.error("接入失败", (error as Error).message);
    }
  };

  const desktopPreview = preview?.desktop;
  const showDesktopPreview = target === "claude-desktop" && desktopPreview;
  const previewPath =
    showDesktopPreview && desktopPreview
      ? desktopPreview.config_path
      : preview?.config_path ?? status?.config_path ?? "";
  const previewSnippet =
    showDesktopPreview && desktopPreview
      ? desktopPreview.snippet
      : preview?.snippet ?? "";
  const previewWarning =
    showDesktopPreview && desktopPreview
      ? desktopPreview.warning
      : preview?.warning;

  const canApply =
    Boolean(selectedModel) &&
    !unavailableModel &&
    (!allowedIdSet || modelChoices.some((item) => item.id === selectedModel)) &&
    (target !== "claude-desktop" || desktopDetected) &&
    (keyMode !== "existing" || Boolean(existingKeyId));

  const issues = (status?.issues ?? []).filter((issue) => {
    if (
      target === "claude-cli" &&
      issue.code === "desktop_not_active"
    ) {
      return false;
    }
    if (
      target === "claude-desktop" &&
      (issue.code === "config_missing" || issue.code === "parse_error")
    ) {
      return false;
    }
    return true;
  });

  return (
    <Modal
      open
      onClose={onClose}
      title={meta.title}
      description={meta.description}
      size="xl"
      footer={
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex gap-2">
            {appliedHere && (
              <Button
                size="sm"
                variant="secondary"
                loading={testMutation.isPending}
                onClick={handleTest}
              >
                <Wifi size={14} />
                测试连接
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={handleOpenDir}>
              <FolderOpen size={14} />
              打开配置目录
            </Button>
          </div>
          <div className="flex gap-2">
            <Button size="sm" variant="secondary" onClick={onClose}>
              取消
            </Button>
            <Button
              size="sm"
              onClick={handleApply}
              loading={applyMutation.isPending}
              disabled={!canApply}
            >
              {appliedHere ? "重新接入" : "接入"}
            </Button>
          </div>
        </div>
      }
    >
      <div className="space-y-5">
        {status?.external_conflict && (
          <Notice tone="warning">
            当前配置已被其他工具（如 cc-switch）接管。保存并接入会覆盖它，写入前会自动备份原配置。
          </Notice>
        )}
        {status?.config_parse_error && (
          <Notice tone="danger">{status.config_parse_error}</Notice>
        )}
        {target === "claude-desktop" && !desktopDetected && (
          <Notice tone="warning">
            未检测到 Claude 桌面端第三方推理配置。请先在桌面端开启 Developer Mode 并配置一次（Help → Troubleshooting → 启用 Developer Mode → Developer → Configure Third-Party Inference）。
          </Notice>
        )}
        {issues.length > 0 && (
          <div className="space-y-2">
            {issues.map((issue, index) => (
              <Notice key={`${issue.code}-${index}`} tone={issue.severity}>
                {issue.message}
              </Notice>
            ))}
          </div>
        )}

        <section>
          <h4 className="mb-2 text-sm font-semibold text-text-primary">
            接入密钥
          </h4>
          <KeySourcePicker
            mode={keyMode}
            onModeChange={setKeyMode}
            name={newKeyName}
            onNameChange={setNewKeyName}
            keys={usableKeys}
            keyId={existingKeyId}
            onKeyIdChange={setExistingKeyId}
            namePlaceholder={meta.keyName}
            nameHint="接入时自动创建；明文只写入客户端配置，不在界面回显。"
          />
        </section>

        <section>
          <h4 className="mb-2 text-sm font-semibold text-text-primary">模型</h4>
          {client === "codex" ? (
            <ModelField
              label="默认模型"
              value={model}
              onChange={setModel}
              customValue={customModel}
              onCustomChange={setCustomModel}
              models={modelChoices}
              allowCustom={!allowedIdSet}
            />
          ) : (
            <div className="space-y-3">
              <ModelField
                label="默认模型（ANTHROPIC_MODEL）"
                value={model}
                onChange={setModel}
                customValue={customModel}
                onCustomChange={setCustomModel}
                models={modelChoices}
                allowCustom={!allowedIdSet}
              />
              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3">
                <ModelField
                  label="Opus 映射"
                  value={opus}
                  onChange={setOpus}
                  customValue={opusCustom}
                  onCustomChange={setOpusCustom}
                  models={modelChoices}
                  allowEmpty
                  allowCustom={!allowedIdSet}
                />
                <ModelField
                  label="Sonnet 映射"
                  value={sonnet}
                  onChange={setSonnet}
                  customValue={sonnetCustom}
                  onCustomChange={setSonnetCustom}
                  models={modelChoices}
                  allowEmpty
                  allowCustom={!allowedIdSet}
                />
                <ModelField
                  label="Haiku 映射"
                  value={haiku}
                  onChange={setHaiku}
                  customValue={haikuCustom}
                  onCustomChange={setHaikuCustom}
                  models={modelChoices}
                  allowEmpty
                  allowCustom={!allowedIdSet}
                />
                <ModelField
                  label="Fable 映射"
                  value={fable}
                  onChange={setFable}
                  customValue={fableCustom}
                  onCustomChange={setFableCustom}
                  models={modelChoices}
                  allowEmpty
                  allowCustom={!allowedIdSet}
                />
              </div>
            </div>
          )}
          {allowedIdSet && (
            <p className="mt-2 text-xs text-info">
              模型选项已按所选密钥的可用模型过滤（{modelChoices.length} 个）。
            </p>
          )}
          {allowedIdSet && modelChoices.length === 0 && (
            <div className="mt-2">
              <Notice tone="warning">
                所选密钥没有可用模型，请更换密钥或在「密钥」页调整允许模型。
              </Notice>
            </div>
          )}
          {unavailableModel && (
            <div className="mt-2">
              <Notice tone="warning">
                模型 `{unavailableModel}` 没有匹配的启用渠道，请从上方下拉重新选择。
              </Notice>
            </div>
          )}
          <p className="mt-2 text-xs text-text-muted">
            {target === "claude-cli"
              ? "本次只更新 Claude Code CLI，不会改动 Claude 桌面端档案。"
              : target === "claude-desktop"
                ? "本次只更新 Claude 桌面端档案，不会改动 CLI 的 settings.json。"
                : "模型必须在至少一个启用渠道中可用。"}
          </p>
        </section>

        <section>
          <h4 className="mb-2 text-sm font-semibold text-text-primary">
            配置预览
          </h4>
          <p className="mb-2 text-xs text-text-muted">
            仅显示将写入 / 覆盖的键；文件中其余配置、注释和其他 provider 保持不变。
          </p>
          {previewWarning && (
            <div className="mb-2">
              <Notice tone="warning">{previewWarning}</Notice>
            </div>
          )}
          <ConfigPreview
            path={previewPath}
            snippet={previewSnippet}
            loading={previewLoading}
          />
        </section>

        {testResult && (
          <Notice tone={testResult.ok ? "success" : "danger"}>
            {testResult.ok
              ? `连接正常 · HTTP ${testResult.status} · ${testResult.latency_ms}ms`
              : testResult.message}
          </Notice>
        )}

        <Notice tone="info">
          接入后请完全退出并重开客户端：桌面 App 使用 Cmd+Q 退出进程，CLI 请结束正在运行的进程并重开终端。
        </Notice>
      </div>
    </Modal>
  );
}
