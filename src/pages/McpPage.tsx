import { Server, Radio, Wrench, BookOpen } from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { Card } from "../components/ui/Card";
import { Badge } from "../components/ui/Badge";
import { Spinner } from "../components/ui/Spinner";
import { EmptyState } from "../components/ui/EmptyState";
import { McpAccessCard } from "../components/mcp/McpAccessCard";
import { McpTestBench } from "../components/mcp/McpTestBench";
import { useServiceStatuses } from "../hooks/useMcp";
import { formatNumber } from "../lib/constants";

export const McpPage = () => {
  const { data: statuses, isLoading } = useServiceStatuses();
  const mcp = statuses?.find((s) => s.id === "mcp");
  const tools = mcp?.stats?.tools ?? [];

  return (
    <div>
      <PageHeader
        title="MCP 服务"
        description="Model Context Protocol Server：对外部 Agent 客户端（Claude Desktop / Cursor 等）暴露知识库工具"
      />

      {/* 服务状态 */}
      {isLoading ? (
        <div className="flex justify-center py-14">
          <Spinner />
        </div>
      ) : !mcp ? (
        <Card>
          <EmptyState
            icon={Server}
            title="MCP 服务未注册"
            description="后端未返回 MCP 服务状态，请确认服务已启动"
          />
        </Card>
      ) : (
        <Card className="mb-5">
          <div className="flex items-start justify-between gap-4">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-accent/15 flex items-center justify-center">
                <Server size={20} className="text-accent" />
              </div>
              <div>
                <h2 className="text-base font-semibold text-text-primary">{mcp.name}</h2>
                <p className="text-xs text-text-secondary">{mcp.description}</p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant={mcp.running ? "success" : "danger"}>
                {mcp.running ? "运行中" : "未运行"}
              </Badge>
              <Badge variant={mcp.enabled ? "info" : "neutral"}>
                {mcp.enabled ? "已启用" : "已禁用"}
              </Badge>
            </div>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mt-4">
            <Stat icon={<Wrench size={14} />} label="工具" value={formatNumber(tools.length)} />
            <Stat
              icon={<BookOpen size={14} />}
              label="可用知识库"
              value={formatNumber(mcp.stats.available_knowledge_bases ?? 0)}
            />
            <Stat icon={<Radio size={14} />} label="Streamable HTTP" value="/mcp" />
            <Stat icon={<Radio size={14} />} label="SSE" value="/mcp/sse" />
          </div>
        </Card>
      )}

      {/* 接入信息 + 测试台 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-5 items-start">
        <McpAccessCard />
        <McpTestBench />
      </div>

      {/* 工具清单 */}
      <Card
        title="工具清单"
        description={`MCP Server 对外暴露的 ${tools.length} 个知识库工具`}
        className="mt-5"
      >
        {tools.length === 0 ? (
          <EmptyState icon={Wrench} title="暂无工具" description="后端未返回工具定义" />
        ) : (
          <ul className="divide-y divide-border-primary">
            {tools.map((t) => (
              <li key={t.name} className="py-3 first:pt-0 last:pb-0">
                <div className="flex items-start gap-3">
                  <code className="text-sm mono font-medium text-accent whitespace-nowrap">
                    {t.name}
                  </code>
                  <p className="text-xs text-text-secondary leading-relaxed">{t.description}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
};

function Stat({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-2 p-2.5 rounded-lg bg-bg-tertiary border border-border-primary">
      <div className="text-text-muted flex-shrink-0">{icon}</div>
      <div className="min-w-0">
        <div className="text-[10px] text-text-muted">{label}</div>
        <div className="text-sm font-semibold text-text-primary tabular truncate">{value}</div>
      </div>
    </div>
  );
}
