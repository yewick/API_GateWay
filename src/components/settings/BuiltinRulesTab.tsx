import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { RotateCcw, AlertTriangle } from "lucide-react";
import type { BuiltinRule, UpdateBuiltinRuleInput } from "../../types";
import { securityApi } from "../../lib/api";
import { Badge } from "../ui/Badge";
import { Table, type Column } from "../ui/Table";
import { Spinner } from "../ui/Spinner";
import { toast } from "../../lib/toast";
import { useState } from "react";

const SEVERITY_COLOR: Record<string, string> = {
  clean: "neutral",
  info: "neutral",
  low: "success",
  medium: "warning",
  high: "danger",
  critical: "danger",
};

const CATEGORY_LABELS: Record<string, string> = {
  credential: "凭证",
  file: "文件",
  infra: "基础设施",
  personal: "个人信息",
  unicode: "Unicode隐写",
  network: "网络风险",
  tool: "工具/命令",
  prompt: "提示词风险",
};

export function BuiltinRulesTab() {
  const qc = useQueryClient();
  const [confirmReset, setConfirmReset] = useState(false);

  const { data: rules, isLoading } = useQuery({
    queryKey: ["builtin-rules"],
    queryFn: securityApi.getBuiltinRules,
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateBuiltinRuleInput }) =>
      securityApi.updateBuiltinRule(id, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["builtin-rules"] });
      toast.success("已更新", "规则已保存");
    },
    onError: (e: Error) => toast.error("更新失败", e.message),
  });

  const resetMutation = useMutation({
    mutationFn: securityApi.resetBuiltinRules,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["builtin-rules"] });
      toast.success("已重置", "内置规则已恢复为默认值");
      setConfirmReset(false);
    },
    onError: (e: Error) => toast.error("重置失败", e.message),
  });

  const columns: Column<BuiltinRule>[] = [
    {
      key: "rule_id",
      title: "规则ID",
      render: (_, r) => (
        <code className="text-xs mono text-text-secondary">{r.rule_id}</code>
      ),
    },
    {
      key: "category",
      title: "类别",
      width: "110px",
      render: (_, r) => (
        <span className="text-xs text-text-secondary">
          {CATEGORY_LABELS[r.category] || r.category}
        </span>
      ),
    },
    {
      key: "severity",
      title: "严重度",
      width: "90px",
      render: (_, r) => (
        <Badge variant={(SEVERITY_COLOR[r.severity] || "neutral") as never}>
          {r.severity}
        </Badge>
      ),
    },
    {
      key: "title",
      title: "标题",
      render: (_, r) => (
        <span className="text-xs text-text-primary">{r.title}</span>
      ),
    },
    {
      key: "description",
      title: "描述",
      render: (_, r) => (
        <span className="text-xs text-text-muted line-clamp-2">
          {r.description || "-"}
        </span>
      ),
    },
    {
      key: "enabled",
      title: "启用",
      width: "70px",
      align: "center",
      render: (_, r) => (
        <input
          type="checkbox"
          className="w-4 h-4 accent-accent cursor-pointer"
          checked={r.enabled === 1}
          onChange={() =>
            updateMutation.mutate({
              id: r.id,
              input: { enabled: r.enabled !== 1 },
            })
          }
        />
      ),
    },
  ];

  if (isLoading) {
    return <Spinner />;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-semibold text-text-primary">
            内置安全规则
          </h4>
          <p className="text-xs text-text-muted">
            共 {rules?.length ?? 0} 条规则。严重度为内置固定值，仅展示；启用开关控制该规则是否生效。
          </p>
        </div>
        {!confirmReset ? (
          <button
            onClick={() => setConfirmReset(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-amber-400 border border-amber-400/30 rounded-lg hover:bg-amber-400/10 transition-colors"
          >
            <RotateCcw size={13} />
            重置为默认
          </button>
        ) : (
          <div className="flex items-center gap-2">
            <AlertTriangle size={14} className="text-amber-400" />
            <span className="text-xs text-amber-400">确认重置？</span>
            <button
              onClick={() => resetMutation.mutate()}
              disabled={resetMutation.isPending}
              className="px-2 py-1 text-xs text-white bg-danger rounded hover:bg-danger/80"
            >
              确认
            </button>
            <button
              onClick={() => setConfirmReset(false)}
              className="px-2 py-1 text-xs text-text-secondary border border-border-primary rounded hover:bg-bg-hover"
            >
              取消
            </button>
          </div>
        )}
      </div>

      <div className="bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
        <Table
          columns={columns}
          data={rules ?? []}
          rowKey={(r) => r.id}
          emptyText="暂无内置规则"
          emptyDescription="数据库尚未初始化安全规则，请重启应用"
          compact
        />
      </div>
    </div>
  );
}
