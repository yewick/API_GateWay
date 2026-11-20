import { FileJson } from "lucide-react";
import { Card } from "../ui/Card";
import { CodeBlock } from "../ui/CodeBlock";
import { EmptyState } from "../ui/EmptyState";
import type { RequestLog } from "../../types";

interface LogRawJsonProps {
  log: RequestLog;
}

function prettyJson(raw: string | null): string {
  if (!raw) return "";
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

export function LogRawJson({ log }: LogRawJsonProps) {
  const hasAny =
    Boolean(log.request_body) ||
    Boolean(log.response_body) ||
    Boolean(log.response_choices) ||
    Boolean(log.forward_body);

  if (!hasAny) {
    return (
      <EmptyState
        icon={FileJson}
        title="没有保存原始 JSON"
        description="该请求没有请求体或响应体，例如内部 embedding 请求"
      />
    );
  }

  return (
    <div className="space-y-4">
      <Card title="请求体" description="客户端原始入站请求（转换前）">
        <div>
          <CodeBlock
            code={prettyJson(log.request_body)}
            title="request_body"
            emptyText="未保存请求体"
            maxHeight={420}
          />
        </div>
      </Card>

      <Card
        title="响应体"
        description="最终返回给客户端的协议响应（新版本流式也会保存）"
      >
        <div>
          <CodeBlock
            code={prettyJson(log.response_body)}
            title="response_body"
            emptyText="旧日志或失败请求未保存响应体"
            maxHeight={420}
          />
        </div>
      </Card>

      {log.response_choices && (
        <Card
          title="响应选择"
          description="上游 OpenAI choices（旧日志的降级来源）"
        >
          <div>
            <CodeBlock
              code={prettyJson(log.response_choices)}
              title="response_choices"
              maxHeight={360}
            />
          </div>
        </Card>
      )}

      {log.forward_body && (
        <Card
          title="脱敏后转发体"
          description="实际发送到上游的请求（仅发生脱敏时保存）"
        >
          <div>
            <CodeBlock
              code={prettyJson(log.forward_body)}
              title="forward_body"
              maxHeight={360}
            />
          </div>
        </Card>
      )}
    </div>
  );
}
