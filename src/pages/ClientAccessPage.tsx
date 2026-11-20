import { useMemo, useState } from "react";
import { Plus, Search } from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { Button } from "../components/ui/Button";
import { Input } from "../components/ui/Input";
import { Card } from "../components/ui/Card";
import { EmptyState } from "../components/ui/EmptyState";
import { Spinner } from "../components/ui/Spinner";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { AccessStatusBar } from "../components/client-access/AccessStatusBar";
import { AddTargetDialog } from "../components/client-access/AddTargetDialog";
import { ClientSetupDialog } from "../components/client-access/ClientSetupDialog";
import { ClientTargetCard } from "../components/client-access/ClientTargetCard";
import { KeepAliveNotice } from "../components/client-access/KeepAliveNotice";
import { useApiKeys } from "../hooks/useApiKeys";
import {
  useClientSetupEvents,
  useClientSetupModels,
  useClientSetupStatus,
  useDisconnectClientSetup,
  useReconcileClientSetup,
  useTestClientSetup,
} from "../hooks/useClientSetup";
import { clientSetupApi } from "../lib/api";
import {
  buildTargetViews,
  matchesTarget,
  type TargetId,
  type TargetView,
} from "../lib/clientAccess";
import { toast } from "../lib/toast";
import type { ClientTestResult } from "../types";

const disconnectCopy: Record<
  TargetId,
  { title: string; description: string }
> = {
  codex: {
    title: "断开 Codex CLI 接入",
    description:
      "将删除 [model_providers.yeapi] 并恢复接入前的 model_provider / model；auth.json 不会被修改。",
  },
  "claude-cli": {
    title: "断开 Claude Code CLI 接入",
    description:
      "只恢复 settings.json 中的 ANTHROPIC_* 配置；Claude 桌面端档案不受影响。",
  },
  "claude-desktop": {
    title: "断开 Claude Desktop 接入",
    description:
      "只切回接入前的桌面端推理档案并恢复模型别名映射；Claude Code CLI 配置不受影响。",
  },
};

export const ClientAccessPage = () => {
  const [search, setSearch] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [editorTarget, setEditorTarget] = useState<TargetId | null>(null);
  const [disconnectTarget, setDisconnectTarget] = useState<TargetView | null>(
    null,
  );
  const [testResults, setTestResults] = useState<
    Partial<Record<TargetId, ClientTestResult>>
  >({});

  const codexStatus = useClientSetupStatus("codex");
  const claudeStatus = useClientSetupStatus("claude");
  const { data: models = [] } = useClientSetupModels();
  const { data: apiKeys = [] } = useApiKeys();
  useClientSetupEvents();

  const testMutation = useTestClientSetup();
  const disconnectMutation = useDisconnectClientSetup();
  const reconcileMutation = useReconcileClientSetup();

  const views = useMemo(
    () => buildTargetViews(codexStatus.data, claudeStatus.data),
    [codexStatus.data, claudeStatus.data],
  );
  const filtered = useMemo(
    () => views.filter((view) => matchesTarget(view, search)),
    [views, search],
  );

  const handleTest = async (view: TargetView) => {
    try {
      const result = await testMutation.mutateAsync(view.client);
      setTestResults((prev) => ({ ...prev, [view.id]: result }));
    } catch (error) {
      toast.error("测试失败", (error as Error).message);
    }
  };

  const handleOpenDir = async (view: TargetView) => {
    try {
      await clientSetupApi.openDir(view.client);
    } catch (error) {
      toast.error("打开失败", (error as Error).message);
    }
  };

  const handleCopyGateway = async (view: TargetView) => {
    if (!view.gatewayUrl) return;
    try {
      await navigator.clipboard.writeText(view.gatewayUrl);
      toast.success("已复制", view.gatewayUrl);
    } catch {
      toast.error("复制失败", "请手动选择并复制");
    }
  };

  const handleRepair = async (view: TargetView) => {
    try {
      const report = await reconcileMutation.mutateAsync({
        client: view.client,
        mode: "repair",
      });
      const result = report.clients[view.client];
      if (result?.actions.length) {
        result.actions.forEach((action) => toast.success("已修复", action));
      } else if (result?.issues.length) {
        toast.warning("仍需处理", result.issues[0].message);
      } else {
        toast.success("状态正常", "没有需要修复的内容");
      }
    } catch (error) {
      toast.error("修复失败", (error as Error).message);
    }
  };

  const handleDisconnect = async () => {
    if (!disconnectTarget) return;
    const scope =
      disconnectTarget.id === "codex"
        ? "all"
        : disconnectTarget.id === "claude-cli"
          ? "cli"
          : "desktop";
    try {
      const result = await disconnectMutation.mutateAsync({
        client: disconnectTarget.client,
        scope,
      });
      result.warnings.forEach((warning) => toast.warning("注意", warning));
      toast.success(
        "已断开接入",
        result.backup_path ? `原配置已备份：${result.backup_path}` : undefined,
      );
    } catch (error) {
      toast.error("断开失败", (error as Error).message);
    } finally {
      setDisconnectTarget(null);
    }
  };

  if (codexStatus.isLoading || claudeStatus.isLoading) {
    return (
      <div className="flex justify-center py-14">
        <Spinner />
      </div>
    );
  }

  if (!codexStatus.data && !claudeStatus.data) {
    return (
      <Card>
        <EmptyState
          icon={Search}
          title="后端不可用"
          description="请在 YeAPI 桌面应用中打开此页面"
        />
      </Card>
    );
  }

  return (
    <div>
      <PageHeader
        title="客户端接入"
        description="把 Codex / Claude Code / Claude 桌面端接到本机网关；接入后自检会自动保持可用"
        actions={
          <>
            <div className="relative">
              <Search
                size={14}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted"
              />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="搜索接入目标…"
                className="w-52 pl-8"
              />
            </div>
            <Button onClick={() => setAddOpen(true)}>
              <Plus size={16} />
              添加接入
            </Button>
          </>
        }
      />

      <KeepAliveNotice />
      <AccessStatusBar views={views} claude={claudeStatus.data} />

      <div className="space-y-2">
        {filtered.map((view) => (
          <ClientTargetCard
            key={view.id}
            view={view}
            testing={
              testMutation.isPending && testMutation.variables === view.client
            }
            testResult={testResults[view.id] ?? null}
            onEdit={() => setEditorTarget(view.id)}
            onTest={() => handleTest(view)}
            onDisconnect={() => setDisconnectTarget(view)}
            onOpenDir={() => handleOpenDir(view)}
            onCopyGateway={() => handleCopyGateway(view)}
            onRepair={() => handleRepair(view)}
          />
        ))}
        {filtered.length === 0 && (
          <Card>
            <EmptyState
              icon={Search}
              title="没有匹配的接入目标"
              description="换个关键词试试，或清空搜索框"
            />
          </Card>
        )}
      </div>

      <AddTargetDialog
        open={addOpen}
        views={views}
        onClose={() => setAddOpen(false)}
        onPick={(target) => {
          setAddOpen(false);
          setEditorTarget(target);
        }}
      />

      <ClientSetupDialog
        target={editorTarget}
        onClose={() => setEditorTarget(null)}
        codexStatus={codexStatus.data}
        claudeStatus={claudeStatus.data}
        models={models}
        apiKeys={apiKeys}
      />

      <ConfirmDialog
        open={Boolean(disconnectTarget)}
        title={
          disconnectTarget ? disconnectCopy[disconnectTarget.id].title : ""
        }
        description={
          disconnectTarget ? disconnectCopy[disconnectTarget.id].description : ""
        }
        confirmText="断开接入"
        danger
        loading={disconnectMutation.isPending}
        onConfirm={handleDisconnect}
        onCancel={() => setDisconnectTarget(null)}
      />
    </div>
  );
};
