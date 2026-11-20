import { Wrench } from "lucide-react";
import { Badge } from "../ui/Badge";
import { CodeBlock } from "../ui/CodeBlock";
import { EmptyState } from "../ui/EmptyState";
import type { ViewToolCall } from "../../lib/logNormalize";

interface LogToolCallsProps {
  toolCalls: ViewToolCall[];
}

function pretty(raw: string): string {
  if (!raw) return "";
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

export function LogToolCalls({ toolCalls }: LogToolCallsProps) {
  if (toolCalls.length === 0) {
    return (
      <EmptyState
        icon={Wrench}
        title="本次请求没有工具调用"
        description="如果客户端使用了工具，调用参数与结果会显示在这里"
      />
    );
  }

  return (
    <div className="space-y-3">
      {toolCalls.map((tool, index) => (
        <div
          key={`${tool.id}-${index}`}
          className={`rounded-xl border bg-bg-secondary p-4 ${
            tool.status === "error"
              ? "border-danger/40"
              : "border-border-primary"
          }`}
        >
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-bg-tertiary text-text-secondary">
              <Wrench size={15} />
            </span>
            <span className="text-sm font-semibold mono text-text-primary">
              {tool.name}
            </span>
            <Badge
              variant={
                tool.status === "success"
                  ? "success"
                  : tool.status === "error"
                    ? "danger"
                    : "warning"
              }
            >
              {tool.status === "success"
                ? "成功"
                : tool.status === "error"
                  ? "失败"
                  : "等待结果"}
            </Badge>
            <span className="text-[11px] text-text-muted mono break-all">
              {tool.id}
            </span>
          </div>

          <div className="mt-3 space-y-2">
            <CodeBlock
              title="参数"
              code={pretty(tool.argumentsRaw)}
              defaultOpen={tool.argumentsRaw.length <= 2048}
              maxHeight={240}
              emptyText="无参数"
            />
            {tool.result ? (
              <div
                className={
                  tool.result.isError
                    ? "rounded-lg border border-danger/40 p-1"
                    : ""
                }
              >
                <CodeBlock
                  title="结果"
                  code={pretty(tool.result.content)}
                  defaultOpen={tool.result.content.length <= 4096}
                  maxHeight={320}
                />
              </div>
            ) : (
              <div className="rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
                尚未看到该工具调用对应的结果（可能在本轮之后或日志范围之外）。
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
