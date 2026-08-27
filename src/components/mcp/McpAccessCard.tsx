import { useState } from "react";
import { Copy, Check } from "lucide-react";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { useSettingsStore } from "../../stores/settingsStore";
import { toast } from "../../lib/toast";

/** 接入信息：展示 MCP 端点并一键复制 Claude Desktop 配置片段 / 端点 URL。 */
export function McpAccessCard() {
  const settings = useSettingsStore((s) => s.settings);
  const endpoint = `http://${settings.server_host}:${settings.server_port}/mcp`;
  const sseEndpoint = `http://${settings.server_host}:${settings.server_port}/mcp/sse`;

  const [copied, setCopied] = useState<"endpoint" | "config" | null>(null);

  const copy = async (key: "endpoint" | "config", text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key);
      toast.success("已复制", label);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      toast.error("复制失败", "请手动复制");
    }
  };

  const configSnippet = JSON.stringify(
    { mcpServers: { yeapi: { url: endpoint } } },
    null,
    2,
  );

  return (
    <Card
      title="接入信息"
      description="在 Claude Desktop / Cursor 等外部 Agent 客户端中接入本机 MCP Server"
    >
      <div className="space-y-4">
        <div>
          <label className="block text-xs font-medium text-text-secondary mb-1">
            Streamable HTTP 端点
          </label>
          <div className="flex items-center gap-2">
            <code className="flex-1 text-xs mono text-text-primary bg-bg-tertiary border border-border-primary rounded-lg px-3 py-2 break-all">
              {endpoint}
            </code>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => copy("endpoint", endpoint, "端点 URL 已复制")}
            >
              {copied === "endpoint" ? <Check size={13} /> : <Copy size={13} />}
              复制
            </Button>
          </div>
          <p className="text-[11px] text-text-muted mt-1">
            传统 SSE 端点：<code className="mono">{sseEndpoint}</code>
          </p>
        </div>

        <div className="pt-3 border-t border-border-primary">
          <div className="flex items-center justify-between mb-1">
            <label className="text-xs font-medium text-text-secondary">
              Claude Desktop 配置片段
            </label>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => copy("config", configSnippet, "Claude Desktop 配置已复制")}
            >
              {copied === "config" ? <Check size={13} /> : <Copy size={13} />}
              一键复制
            </Button>
          </div>
          <pre className="text-xs mono text-text-primary bg-bg-tertiary rounded-lg p-3 overflow-x-auto">
            {configSnippet}
          </pre>
        </div>
      </div>
    </Card>
  );
}
