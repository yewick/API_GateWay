import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import type { CustomRule, CreateCustomRuleInput } from "../../types";
import { securityApi } from "../../lib/api";
import { Badge } from "../ui/Badge";
import { Table, type Column } from "../ui/Table";
import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { Spinner } from "../ui/Spinner";
import { toast } from "../../lib/toast";

const SEVERITY_COLOR: Record<string, string> = {
  low: "success",
  medium: "warning",
  high: "danger",
  critical: "danger",
};

const RULE_TYPE_LABELS: Record<string, string> = {
  blacklist: "黑名单",
  whitelist: "白名单",
};

const CATEGORY_OPTIONS = ["keyword", "domain", "tool", "path"];
const SEVERITY_OPTIONS = ["low", "medium", "high", "critical"];
const ACTION_OPTIONS = ["warn", "block"];

const CATEGORY_LABELS: Record<string, string> = {
  keyword: "keyword（关键词）",
  domain: "domain（域名）",
  tool: "tool（工具/命令）",
  path: "path（路径）",
};

const ACTION_LABELS: Record<string, string> = {
  warn: "warn（按模式告警/放行）",
  block: "block（命中即阻断 451）",
};

const emptyForm: CreateCustomRuleInput = {
  rule_type: "blacklist",
  category: "keyword",
  pattern: "",
  severity: "medium",
  action: "warn",
  description: "",
};

export function CustomRulesTab() {
  const qc = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<CreateCustomRuleInput>({ ...emptyForm });

  const { data: rules, isLoading } = useQuery({
    queryKey: ["custom-rules"],
    queryFn: securityApi.getCustomRules,
  });

  const createMutation = useMutation({
    mutationFn: (input: CreateCustomRuleInput) => securityApi.createCustomRule(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["custom-rules"] });
      toast.success("已创建", "自定义规则已添加");
      setShowForm(false);
      setForm({ ...emptyForm });
    },
    onError: (e: Error) => toast.error("创建失败", e.message),
  });

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      securityApi.toggleCustomRule(id, enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["custom-rules"] }),
    onError: (e: Error) => toast.error("操作失败", e.message),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => securityApi.deleteCustomRule(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["custom-rules"] });
      toast.success("已删除");
    },
    onError: (e: Error) => toast.error("删除失败", e.message),
  });

  const handleSubmit = () => {
    if (!form.pattern.trim()) {
      toast.error("请输入匹配内容", "pattern 不能为空");
      return;
    }
    createMutation.mutate(form);
  };

  const columns: Column<CustomRule>[] = [
    {
      key: "rule_type",
      title: "类型",
      width: "80px",
      render: (_, r) => (
        <Badge variant={r.rule_type === "blacklist" ? "danger" : "success"}>
          {RULE_TYPE_LABELS[r.rule_type] || r.rule_type}
        </Badge>
      ),
    },
    {
      key: "category",
      title: "类别",
      width: "90px",
      render: (_, r) => (
        <span className="text-xs text-text-secondary">{r.category}</span>
      ),
    },
    {
      key: "pattern",
      title: "匹配内容",
      render: (_, r) => (
        <code className="text-xs mono text-text-primary bg-bg-tertiary px-1.5 py-0.5 rounded">
          {r.pattern}
        </code>
      ),
    },
    {
      key: "severity",
      title: "严重度",
      width: "80px",
      render: (_, r) => (
        <Badge variant={(SEVERITY_COLOR[r.severity] || "neutral") as never}>
          {r.severity}
        </Badge>
      ),
    },
    {
      key: "action",
      title: "动作",
      width: "70px",
      render: (_, r) => (
        <span className="text-xs text-text-secondary">{r.action || "warn"}</span>
      ),
    },
    {
      key: "enabled",
      title: "启用",
      width: "60px",
      align: "center",
      render: (_, r) => (
        <input
          type="checkbox"
          className="w-4 h-4 accent-accent cursor-pointer"
          checked={r.enabled === 1}
          onChange={() =>
            toggleMutation.mutate({ id: r.id, enabled: r.enabled !== 1 })
          }
        />
      ),
    },
    {
      key: "actions",
      title: "操作",
      width: "60px",
      align: "center",
      render: (_, r) => (
        <button
          onClick={(e) => {
            e.stopPropagation();
            deleteMutation.mutate(r.id);
          }}
          className="p-1 text-text-muted hover:text-danger transition-colors"
          title="删除规则"
        >
          <Trash2 size={14} />
        </button>
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
            自定义规则
          </h4>
          <p className="text-xs text-text-muted">
            共 {rules?.length ?? 0} 条规则。黑名单命中后产生 finding；白名单仅豁免内置规则，不覆盖同义黑名单（黑名单 block 命中即阻断）。
          </p>
        </div>
        <Button onClick={() => setShowForm(true)} size="sm">
          <Plus size={14} />
          添加规则
        </Button>
      </div>

      <div className="bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
        <Table
          columns={columns}
          data={rules ?? []}
          rowKey={(r) => r.id}
          emptyText="暂无自定义规则"
          emptyDescription="点击「添加规则」创建黑名单或白名单"
          compact
        />
      </div>

      {/* 添加规则表单 */}
      <Modal
        open={showForm}
        onClose={() => setShowForm(false)}
        title="添加自定义规则"
        description="创建黑名单/白名单规则，匹配内容使用子串匹配"
        size="sm"
        footer={
          <div className="flex gap-2 justify-end">
            <Button variant="ghost" onClick={() => setShowForm(false)}>
              取消
            </Button>
            <Button onClick={handleSubmit} loading={createMutation.isPending}>
              创建
            </Button>
          </div>
        }
      >
        <div className="space-y-4">
          {/* 规则类型 */}
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              规则类型
            </label>
            <select
              className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
              value={form.rule_type}
              onChange={(e) => setForm({ ...form, rule_type: e.target.value })}
            >
              <option value="blacklist">黑名单（匹配后告警/阻断）</option>
              <option value="whitelist">白名单（匹配后豁免检测）</option>
            </select>
          </div>

          {/* 类别 */}
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              类别
            </label>
            <select
              className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
              value={form.category}
              onChange={(e) => setForm({ ...form, category: e.target.value })}
            >
              {CATEGORY_OPTIONS.map((c) => (
                <option key={c} value={c}>{CATEGORY_LABELS[c] || c}</option>
              ))}
            </select>
            <p className="text-xs text-text-muted mt-1">
              白名单类别决定豁免哪类检测：domain→网络、tool→工具、path→文件/路径、keyword→全部
            </p>
          </div>

          {/* 匹配内容 */}
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              匹配内容 <span className="text-danger">*</span>
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary placeholder-text-muted outline-none focus:border-accent"
              placeholder="例如：公司内部关键词、敏感域名"
              value={form.pattern}
              onChange={(e) => setForm({ ...form, pattern: e.target.value })}
            />
          </div>

          {/* 严重度 + 动作 */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">
                严重度
              </label>
              <select
                className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                value={form.severity}
                onChange={(e) => setForm({ ...form, severity: e.target.value })}
              >
                {SEVERITY_OPTIONS.map((s) => (
                  <option key={s} value={s}>{s}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">
                动作
              </label>
              <select
                className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                value={form.action}
                onChange={(e) => setForm({ ...form, action: e.target.value })}
              >
                {ACTION_OPTIONS.map((a) => (
                  <option key={a} value={a}>{ACTION_LABELS[a] || a}</option>
                ))}
              </select>
            </div>
          </div>

          {/* 描述 */}
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              描述（可选）
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary placeholder-text-muted outline-none focus:border-accent"
              placeholder="规则说明"
              value={form.description || ""}
              onChange={(e) => setForm({ ...form, description: e.target.value })}
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
