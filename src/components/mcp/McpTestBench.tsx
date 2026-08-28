import { useState } from "react";
import { Plug, ListTree, Wrench } from "lucide-react";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { useSettingsStore } from "../../stores/settingsStore";
import { useServiceStatuses } from "../../hooks/useMcp";
import { mcpApi } from "../../lib/api";

interface McpResult {
  status: number;
  body: unknown;
}

/** MCP 测试台：直连 /mcp，调试 initialize / tools/list / tools/call 三类 JSON-RPC 请求。 */
export function McpTestBench() {
  const settings = useSettingsStore((s) => s.settings);
  const { data: statuses } = useServiceStatuses();
  const mcp = statuses?.find((s) => s.id === "mcp");
  const tools = mcp?.stats?.tools ?? [];

  const [host, setHost] = useState(
    `http://${settings.server_host}:${settings.server_port}`,
  );
  const [toolName, setToolName] = useState("");
  const [args, setArgs] = useState("{}");
  const [lastAction, setLastAction] = useState("");
  const [result, setResult] = useState<McpResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  const run = async (action: string, method: string, params: unknown) => {
    setSending(true);
    setLastAction(action);
    setResult(null);
    setError(null);
    try {
      const res = await mcpApi.sendRequest({ host, method, params });
      setResult(res);
    } catch (e) {
      setError((e as Error)?.message ?? String(e));
    } finally {
      setSending(false);
    }
  };

  const handleInitialize = () =>
    run("initialize", "initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "yeapi-console", version: "0.1.5" },
    });

  const handleToolsList = () => run("tools/list", "tools/list", {});

  const handleToolsCall = () => {
    let arguments_;
    try {
      arguments_ = JSON.parse(args);
    } catch {
      setError("参数不是合法 JSON，请修正 arguments 后再调用");
      setResult(null);
      return;
    }
    if (!toolName) {
      setError("请先选择要调用的工具");
      setResult(null);
      return;
    }
    run(`tools/call ${toolName}`, "tools/call", { name: toolName, arguments: arguments_ });
  };

  return (
    <Card
      title="测试台"
      description="直连 /mcp，验证 initialize / tools/list / tools/call 的 JSON-RPC 响应"
    >
      <div className="space-y-4">
        <div>
          <label className="block text-xs font-medium text-text-secondary mb-1">
            地址
          </label>
          <input
            type="text"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
            placeholder="http://127.0.0.1:8777"
          />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={handleInitialize} loading={sending && lastAction === "initialize"}>
            <Plug size={14} />
            initialize
          </Button>
          <Button
            variant="secondary"
            onClick={handleToolsList}
            loading={sending && lastAction === "tools/list"}
          >
            <ListTree size={14} />
            tools/list
          </Button>
        </div>

        {/* 响应结果：紧贴按钮，便于即时看到返回 */}
        {(result || error) && (
          <div className="pt-3 border-t border-border-primary">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs font-medium text-text-secondary">响应</span>
              {lastAction && (
                <span className="text-xs mono text-text-muted">· {lastAction}</span>
              )}
              {result && (
                <span
                  className={`text-xs mono ${
                    result.status >= 200 && result.status < 300
                      ? "text-green-400"
                      : "text-danger"
                  }`}
                >
                  {result.status}
                </span>
              )}
              {error && <span className="text-xs mono text-danger">请求失败</span>}
            </div>
            <pre className="text-xs mono text-text-primary bg-bg-tertiary rounded-lg p-3 overflow-x-auto max-h-72 whitespace-pre-wrap break-all">
              {error ? error : JSON.stringify(result?.body, null, 2)}
            </pre>
          </div>
        )}

        <div className="pt-3 border-t border-border-primary">
          <label className="block text-xs font-medium text-text-secondary mb-1">
            tools/call
          </label>
          <div className="grid grid-cols-2 gap-3 mb-2">
            <select
              value={toolName}
              onChange={(e) => setToolName(e.target.value)}
              className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent"
            >
              <option value="">选择工具</option>
              {tools.map((t) => (
                <option key={t.name} value={t.name}>
                  {t.name}
                </option>
              ))}
            </select>
            <Button
              variant="secondary"
              onClick={handleToolsCall}
              loading={sending && lastAction.startsWith("tools/call")}
            >
              <Wrench size={14} />
              调用
            </Button>
          </div>
          <textarea
            value={args}
            onChange={(e) => setArgs(e.target.value)}
            rows={5}
            className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent resize-y font-mono"
            placeholder='{"query": "示例问题", "top_k": 3}'
          />
        </div>
      </div>
    </Card>
  );
}
