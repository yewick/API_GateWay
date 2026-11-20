import { useEffect, useMemo, useState } from "react";
import { Cable } from "lucide-react";
import { Card } from "../ui/Card";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { EmptyState } from "../ui/EmptyState";
import { Spinner } from "../ui/Spinner";
import { useApiKeys } from "../../hooks/useApiKeys";
import {
  useApplyClientSetup,
  useClientSetupModels,
  useClientSetupPreview,
  useClientSetupStatus,
  useDisconnectClientSetup,
  useTestClientSetup,
} from "../../hooks/useClientSetup";
import { clientSetupApi } from "../../lib/api";
import { toast } from "../../lib/toast";
import type { ApplyClientInput, ClientTestResult } from "../../types";
import { ClaudeCodeSetupCard } from "./ClaudeCodeSetupCard";
import { Notice } from "./SetupInfo";

const aliasValue = (selection: string, custom: string) =>
  selection === "__custom__" ? custom.trim() : selection;

export function ClaudeSetupSection() {
  const { data: status, isLoading } = useClientSetupStatus("claude");
  const { data: models = [] } = useClientSetupModels();
  const { data: apiKeys = [] } = useApiKeys();

  const applyMutation = useApplyClientSetup();
  const disconnectMutation = useDisconnectClientSetup();
  const testMutation = useTestClientSetup();

  const [keyMode, setKeyMode] = useState<"new" | "existing">("new");
  const [newKeyName, setNewKeyName] = useState("Claude Code");
  const [existingKeyId, setExistingKeyId] = useState("");
  const [mainModel, setMainModel] = useState("");
  const [mainCustomModel, setMainCustomModel] = useState("");
  const [opusModel, setOpusModel] = useState("");
  const [opusCustomModel, setOpusCustomModel] = useState("");
  const [sonnetModel, setSonnetModel] = useState("");
  const [sonnetCustomModel, setSonnetCustomModel] = useState("");
  const [haikuModel, setHaikuModel] = useState("");
  const [haikuCustomModel, setHaikuCustomModel] = useState("");
  const [applyCli, setApplyCli] = useState(true);
  const [applyDesktop, setApplyDesktop] = useState(true);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const [testResult, setTestResult] = useState<ClientTestResult | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);
  const [disconnectedWarnings, setDisconnectedWarnings] = useState<string[]>([]);

  const usableKeys = useMemo(
    () =>
      apiKeys.filter(
        (key) =>
          key.status === 1 &&
          (!key.expires_at || new Date(key.expires_at).getTime() > Date.now()),
      ),
    [apiKeys],
  );

  useEffect(() => {
    if (mainModel || !status) return;
    if (status.current_model) {
      setMainModel(status.current_model);
      return;
    }
    if (models.length > 0) {
      setMainModel(models[0].id);
    }
  }, [mainModel, status, models]);

  const desktopDetected = status?.claude?.desktop.detected ?? false;
  const effectiveApplyDesktop = applyDesktop && desktopDetected;
  const selectedMainModel =
    mainModel === "__custom__" ? mainCustomModel.trim() : mainModel;

  const claudeOptions = useMemo(
    () => ({
      opus_model: aliasValue(opusModel, opusCustomModel),
      sonnet_model: aliasValue(sonnetModel, sonnetCustomModel),
      haiku_model: aliasValue(haikuModel, haikuCustomModel),
    }),
    [opusModel, opusCustomModel, sonnetModel, sonnetCustomModel, haikuModel, haikuCustomModel],
  );

  const previewInput = useMemo<ApplyClientInput | null>(() => {
    if (!selectedMainModel) return null;
    return {
      client: "claude",
      key: { mode: "new", name: "preview" },
      model: selectedMainModel,
      claude: claudeOptions,
    };
  }, [selectedMainModel, claudeOptions]);

  const { data: preview, isFetching: previewLoading } =
    useClientSetupPreview(previewInput);

  const handleApply = async () => {
    if (!selectedMainModel) {
      toast.error("缺少模型", "请选择或输入 Claude Code 的默认模型");
      return;
    }
    if (!applyCli && !effectiveApplyDesktop) {
      toast.error("缺少接入范围", "请至少勾选 CLI 或桌面端");
      return;
    }
    if (keyMode === "existing" && !existingKeyId) {
      toast.error("缺少密钥", "请选择一个已有密钥");
      return;
    }

    const input: ApplyClientInput = {
      client: "claude",
      key:
        keyMode === "new"
          ? {
              mode: "new",
              name: newKeyName.trim() || "Claude Code",
              allowed_models: [],
              quota_limit: -1,
            }
          : { mode: "existing", id: existingKeyId },
      model: selectedMainModel,
      claude: {
        ...claudeOptions,
        apply_cli: applyCli,
        apply_desktop: effectiveApplyDesktop,
      },
    };

    try {
      const result = await applyMutation.mutateAsync(input);
      setRestartRequired(result.restart_required);
      setTestResult(null);
      setDisconnectedWarnings([]);
      toast.success(
        "接入完成",
        result.backup_path
          ? `原配置已备份：${result.backup_path}`
          : "已写入 Claude Code 配置",
      );
    } catch (error) {
      toast.error("接入失败", (error as Error).message);
    }
  };

  const handleDisconnect = async () => {
    try {
      const result = await disconnectMutation.mutateAsync("claude");
      setRestartRequired(false);
      setTestResult(null);
      setDisconnectedWarnings(result.warnings);
      result.warnings.forEach((warning) => toast.warning("注意", warning));
      toast.success(
        "已断开接入",
        result.backup_path ? `原配置备份：${result.backup_path}` : undefined,
      );
    } catch (error) {
      toast.error("断开失败", (error as Error).message);
    } finally {
      setConfirmDisconnect(false);
    }
  };

  const handleTest = async () => {
    try {
      setTestResult(await testMutation.mutateAsync("claude"));
    } catch (error) {
      toast.error("测试失败", (error as Error).message);
    }
  };

  const handleOpenDir = async () => {
    try {
      await clientSetupApi.openDir("claude");
    } catch (error) {
      toast.error("打开失败", (error as Error).message);
    }
  };

  if (isLoading) {
    return (
      <div className="flex justify-center py-14">
        <Spinner />
      </div>
    );
  }

  if (!status) {
    return (
      <Card>
        <EmptyState
          icon={Cable}
          title="后端不可用"
          description="请在 YeAPI 桌面应用中打开此页面"
        />
      </Card>
    );
  }

  return (
    <>
      {status.issues.length > 0 && (
        <div className="space-y-2 mb-4">
          {status.issues.map((item, index) => (
            <Notice key={`${item.code}-${index}`} tone={item.severity}>
              {item.message}
            </Notice>
          ))}
        </div>
      )}

      <ClaudeCodeSetupCard
        status={status}
        models={models}
        apiKeys={usableKeys}
        keyMode={keyMode}
        onKeyModeChange={setKeyMode}
        newKeyName={newKeyName}
        onNewKeyNameChange={setNewKeyName}
        existingKeyId={existingKeyId}
        onExistingKeyIdChange={setExistingKeyId}
        mainModel={mainModel}
        onMainModelChange={setMainModel}
        mainCustomModel={mainCustomModel}
        onMainCustomModelChange={setMainCustomModel}
        opusModel={opusModel}
        onOpusModelChange={setOpusModel}
        opusCustomModel={opusCustomModel}
        onOpusCustomModelChange={setOpusCustomModel}
        sonnetModel={sonnetModel}
        onSonnetModelChange={setSonnetModel}
        sonnetCustomModel={sonnetCustomModel}
        onSonnetCustomModelChange={setSonnetCustomModel}
        haikuModel={haikuModel}
        onHaikuModelChange={setHaikuModel}
        haikuCustomModel={haikuCustomModel}
        onHaikuCustomModelChange={setHaikuCustomModel}
        applyCli={applyCli}
        onApplyCliChange={setApplyCli}
        applyDesktop={applyDesktop}
        onApplyDesktopChange={setApplyDesktop}
        desktopDetected={desktopDetected}
        preview={preview}
        previewLoading={previewLoading}
        applying={applyMutation.isPending}
        testing={testMutation.isPending}
        onApply={handleApply}
        onDisconnect={() => setConfirmDisconnect(true)}
        onTest={handleTest}
        onOpenDir={handleOpenDir}
        testResult={testResult}
        restartRequired={restartRequired}
        disconnectedWarnings={disconnectedWarnings}
      />
      <ConfirmDialog
        open={confirmDisconnect}
        title="断开 Claude Code 接入"
        description="将移除 settings.json 中的 ANTHROPIC_* 接入配置并恢复原值；桌面端会切回此前应用的推理档案。其他设置不受影响。断开后请重启 Claude Code 与桌面端使配置生效。"
        confirmText="断开接入"
        danger
        loading={disconnectMutation.isPending}
        onConfirm={handleDisconnect}
        onCancel={() => setConfirmDisconnect(false)}
      />
    </>
  );
}
