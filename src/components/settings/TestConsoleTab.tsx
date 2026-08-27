import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2, Copy, Send } from "lucide-react";
import { useApiKeys } from "../../hooks/useApiKeys";
import { useSettingsStore } from "../../stores/settingsStore";
import { logKeys } from "../../hooks/useLogs";
import { dashboardKeys } from "../../hooks/useDashboard";
import { testApi } from "../../lib/api";
import { Button } from "../ui/Button";
import { toast } from "../../lib/toast";
import type { ApiKey } from "../../types";

interface TestItem {
  id: string;
  name: string;
  apiKeyId: string;
  model: string;
  content: string;
  host: string;
  result: { status: number; body: unknown } | null;
  sending: boolean;
}

const newId = () =>
  Math.random().toString(36).slice(2) + Date.now().toString(36);

// 单引号包裹 + 内部单引号转义，防止生成 curl 时的命令注入
const shellQuote = (s: string) => "'" + s.replace(/'/g, "'\\''") + "'";

function buildCurl(item: TestItem, apiKey: ApiKey | undefined): string {
  const host = item.host.trim().replace(/\/+$/, "");
  const body = JSON.stringify({
    model: item.model,
    messages: [{ role: "user", content: item.content }],
  });
  const auth = "Authorization: Bearer " + (apiKey?.key ?? "");
  return [
    `curl ${shellQuote(host + "/v1/chat/completions")} \\`,
    `  -H ${shellQuote(auth)} \\`,
    `  -H ${shellQuote("Content-Type: application/json")} \\`,
    `  -d ${shellQuote(body)}`,
  ].join("\n");
}

export function TestConsoleTab() {
  const qc = useQueryClient();
  const { data: apiKeys } = useApiKeys();
  const settings = useSettingsStore((s) => s.settings);
  const defaultHost = `http://${settings.server_host}:${settings.server_port}`;

  const [items, setItems] = useState<TestItem[]>(() => [
    {
      id: newId(),
      name: "测试项 1",
      apiKeyId: "",
      model: "",
      content: "",
      host: defaultHost,
      result: null,
      sending: false,
    },
  ]);

  const enabledKeys = (apiKeys ?? []).filter((k) => k.status === 1);

  const updateItem = (id: string, partial: Partial<TestItem>) =>
    setItems((prev) => prev.map((it) => (it.id === id ? { ...it, ...partial } : it)));

  const addItem = () =>
    setItems((prev) => [
      ...prev,
      {
        id: newId(),
        name: `测试项 ${prev.length + 1}`,
        apiKeyId: "",
        model: "",
        content: "",
        host: defaultHost,
        result: null,
        sending: false,
      },
    ]);

  const removeItem = (id: string) =>
    setItems((prev) => prev.filter((it) => it.id !== id));

  const selectedKey = (item: TestItem) =>
    enabledKeys.find((k) => k.id === item.apiKeyId);

  const handleSend = async (item: TestItem) => {
    const key = selectedKey(item);
    if (!key) {
      toast.error("请选择 API Key");
      return;
    }
    if (!item.model.trim()) {
      toast.error("请选择或输入模型");
      return;
    }
    updateItem(item.id, { sending: true, result: null });
    try {
      const res = await testApi.send({
        host: item.host,
        api_key: key.key,
        model: item.model,
        content: item.content,
      });
      updateItem(item.id, { result: res, sending: false });
      // 网关已记录本次请求日志，失效日志/统计/仪表盘，让相关页面即时刷新
      qc.invalidateQueries({ queryKey: logKeys.all });
      qc.invalidateQueries({ queryKey: ["log-stats"] });
      qc.invalidateQueries({ queryKey: ["log-mode-stats"] });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    } catch (e) {
      updateItem(item.id, { sending: false });
      toast.error("发送失败", (e as Error)?.message);
    }
  };

  const handleCopy = async (item: TestItem) => {
    const curl = buildCurl(item, selectedKey(item));
    try {
      await navigator.clipboard.writeText(curl);
      toast.success("已复制", "curl 命令已复制到剪贴板");
    } catch {
      toast.error("复制失败", "请手动复制命令");
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-semibold text-text-primary">测试台</h4>
          <p className="text-xs text-text-muted">
            构造 chat/completions 请求，生成 curl 或在应用内发送；发送后到「日志」页查看「脱敏后转发体」。
          </p>
        </div>
        <Button onClick={addItem} size="sm">
          <Plus size={14} />
          添加测试项
        </Button>
      </div>

      {items.map((item) => {
        const key = selectedKey(item);
        const models = key?.allowed_models ?? [];
        return (
          <div
            key={item.id}
            className="bg-bg-secondary border border-border-primary rounded-xl p-4 space-y-4"
          >
            {/* 标题行 */}
            <div className="flex items-center justify-between">
              <input
                type="text"
                value={item.name}
                onChange={(e) => updateItem(item.id, { name: e.target.value })}
                className="text-sm font-medium text-text-primary bg-transparent border-b border-transparent focus:border-accent outline-none"
                placeholder="测试项名称"
              />
              <button
                onClick={() => removeItem(item.id)}
                className="p-1.5 text-text-muted hover:text-danger transition-colors"
                title="删除测试项"
              >
                <Trash2 size={14} />
              </button>
            </div>

            {/* 表单 */}
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  API Key
                </label>
                <select
                  value={item.apiKeyId}
                  onChange={(e) => {
                    const apiKeyId = e.target.value;
                    const k = enabledKeys.find((x) => x.id === apiKeyId);
                    updateItem(item.id, {
                      apiKeyId,
                      model: k?.allowed_models?.[0] ?? "",
                    });
                  }}
                  className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                >
                  <option value="">选择 API Key</option>
                  {enabledKeys.map((k) => (
                    <option key={k.id} value={k.id}>
                      {k.name}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  模型
                </label>
                {models.length > 0 ? (
                  <select
                    value={item.model}
                    onChange={(e) => updateItem(item.id, { model: e.target.value })}
                    className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                  >
                    {models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    type="text"
                    value={item.model}
                    onChange={(e) => updateItem(item.id, { model: e.target.value })}
                    className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                    placeholder="模型名（该 Key 未声明可选模型）"
                  />
                )}
              </div>
            </div>

            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">
                地址
              </label>
              <input
                type="text"
                value={item.host}
                onChange={(e) => updateItem(item.id, { host: e.target.value })}
                className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
                placeholder="http://127.0.0.1:8777"
              />
            </div>

            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">
                内容
              </label>
              <textarea
                value={item.content}
                onChange={(e) => updateItem(item.id, { content: e.target.value })}
                rows={3}
                className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent resize-y"
                placeholder="输入要发送的消息内容"
              />
            </div>

            {/* 生成的 curl */}
            <div>
              <div className="flex items-center justify-between mb-1">
                <label className="text-xs font-medium text-text-secondary">
                  生成的 curl 命令
                </label>
                <Button variant="ghost" size="sm" onClick={() => handleCopy(item)}>
                  <Copy size={13} />
                  复制
                </Button>
              </div>
              <pre className="text-xs mono text-text-primary bg-bg-tertiary rounded-lg p-3 overflow-x-auto">
                {buildCurl(item, key)}
              </pre>
            </div>

            {/* 发送 + 结果 */}
            <div className="flex items-center gap-3">
              <Button onClick={() => handleSend(item)} loading={item.sending}>
                <Send size={14} />
                发送
              </Button>
              <span className="text-xs text-text-muted">
                发送后到「日志」页查看该请求的「脱敏后转发体」。
              </span>
            </div>

            {item.result && (
              <div className="mt-2">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-xs font-medium text-text-secondary">
                    响应状态
                  </span>
                  <span
                    className={`text-xs mono ${
                      item.result.status >= 200 && item.result.status < 300
                        ? "text-green-400"
                        : "text-danger"
                    }`}
                  >
                    {item.result.status}
                  </span>
                </div>
                <pre className="text-xs mono text-text-primary bg-bg-tertiary rounded-lg p-3 overflow-x-auto max-h-64">
                  {JSON.stringify(item.result.body, null, 2)}
                </pre>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
