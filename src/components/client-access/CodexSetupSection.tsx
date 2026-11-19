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
import { CodexSetupCard } from "./CodexSetupCard";

export function CodexSetupSection() {
  const { data: status, isLoading } = useClientSetupStatus("codex");
  const { data: models = [] } = useClientSetupModels();
  const { data: apiKeys = [] } = useApiKeys();

  const applyMutation = useApplyClientSetup();
  const disconnectMutation = useDisconnectClientSetup();
  const testMutation = useTestClientSetup();

  const [keyMode, setKeyMode] = useState<"new" | "existing">("new");
  const [newKeyName, setNewKeyName] = useState("Codex");
  const [existingKeyId, setExistingKeyId] = useState("");
  const [model, setModel] = useState("");
  const [customModel, setCustomModel] = useState("");
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
    if (model || !status) return;
    if (status.current_model) {
      setModel(status.current_model);
      return;
    }
    if (models.length > 0) {
      setModel(models[0].id);
    }
  }, [model, status, models]);

  const selectedModel = model === "__custom__" ? customModel.trim() : model;

  const previewInput = useMemo<ApplyClientInput | null>(() => {
    if (!selectedModel) return null;
    return {
      client: "codex",
      key: { mode: "new", name: "preview" },
      model: selectedModel,
    };
  }, [selectedModel]);

  const { data: preview, isFetching: previewLoading } =
    useClientSetupPreview(previewInput);

  const handleApply = async () => {
    if (!selectedModel) {
      toast.error("缺少模型", "请选择或输入 Codex 的默认模型");
      return;
    }
    if (keyMode === "existing" && !existingKeyId) {
      toast.error("缺少密钥", "请选择一个已有密钥");
      return;
    }

    const input: ApplyClientInput = {
      client: "codex",
      key:
        keyMode === "new"
          ? {
              mode: "new",
              name: newKeyName.trim() || "Codex",
              allowed_models: [],
              quota_limit: -1,
            }
          : { mode: "existing", id: existingKeyId },
      model: selectedModel,
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
          : "已写入 ~/.codex/config.toml",
      );
    } catch (error) {
      toast.error("接入失败", (error as Error).message);
    }
  };

  const handleDisconnect = async () => {
    try {
      const result = await disconnectMutation.mutateAsync("codex");
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
      setTestResult(await testMutation.mutateAsync("codex"));
    } catch (error) {
      toast.error("测试失败", (error as Error).message);
    }
  };

  const handleOpenDir = async () => {
    try {
      await clientSetupApi.openDir("codex");
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
      <CodexSetupCard
        status={status}
        models={models}
        apiKeys={usableKeys}
        keyMode={keyMode}
        onKeyModeChange={setKeyMode}
        newKeyName={newKeyName}
        onNewKeyNameChange={setNewKeyName}
        existingKeyId={existingKeyId}
        onExistingKeyIdChange={setExistingKeyId}
        model={model}
        onModelChange={setModel}
        customModel={customModel}
        onCustomModelChange={setCustomModel}
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
        title="断开 Codex 接入"
        description="将删除 [model_providers.yeapi] 并恢复接入前的 model_provider / model；auth.json 不会被修改。"
        confirmText="断开接入"
        danger
        loading={disconnectMutation.isPending}
        onConfirm={handleDisconnect}
        onCancel={() => setConfirmDisconnect(false)}
      />
    </>
  );
}
