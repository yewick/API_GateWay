import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { RequestLog, SecurityFinding } from "../../types";
import { formatTime, statusColor } from "../../lib/constants";
import { Modal } from "../ui/Modal";
import { Badge } from "../ui/Badge";
import { useTheme } from "../../hooks/useTheme";
import { useQuery } from "@tanstack/react-query";
import { logApi } from "../../lib/api";

interface LogDetailProps {
  log: RequestLog | null;
  onClose: () => void;
}

// 尝试 JSON 格式化，失败则原样返回
const prettyJson = (raw: string): string => {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
};

function DetailRow({ label, value, mono }: { label: string; value: React.ReactNode; mono?: boolean }) {
  return (
    <div className="flex items-start justify-between py-2 border-b border-border-primary/50 last:border-0">
      <span className="text-xs text-text-muted w-28 flex-shrink-0">{label}</span>
      <span className={`text-xs text-text-primary text-right break-all ${mono ? "mono" : ""}`}>
        {value ?? "-"}
      </span>
    </div>
  );
}

export function LogDetail({ log, onClose }: LogDetailProps) {
  const { isDark } = useTheme();

  const { data: findings } = useQuery({
    queryKey: ["log-findings", log?.id],
    queryFn: () => logApi.getFindings(log!.id),
    enabled: !!log,
  });

  if (!log) return null;

  return (
    <Modal
      open={!!log}
      onClose={onClose}
      title="请求日志详情"
      description={`ID: ${log.id}`}
      size="lg"
    >
      <div className="space-y-6">
        {/* 基本信息 */}
        <div>
          <h4 className="text-sm font-semibold text-text-primary mb-2">基本信息</h4>
          <div className="grid grid-cols-2 gap-x-6">
            <DetailRow label="时间" value={formatTime(log.created_at)} />
            <DetailRow label="密钥" value={log.api_key_name} />
            <DetailRow label="渠道" value={log.channel_name} />
            <DetailRow label="模型" value={log.model} mono />
            <DetailRow label="上游模型" value={log.upstream_model} mono />
            <DetailRow label="模式" value={log.mode} />
            <DetailRow label="Trace ID" value={log.trace_id} mono />
            <DetailRow
              label="状态码"
              value={
                <Badge variant={statusColor(log.status_code) as never}>
                  {log.status_code}
                </Badge>
              }
            />
            <DetailRow label="流式" value={log.is_stream ? "是" : "否"} />
          </div>
        </div>

        {/* Token 与延迟 */}
        <div>
          <h4 className="text-sm font-semibold text-text-primary mb-2">Token 与延迟</h4>
          <div className="grid grid-cols-2 gap-x-6">
            <DetailRow label="提示词 Tokens" value={log.prompt_tokens.toLocaleString()} mono />
            <DetailRow label="补全 Tokens" value={log.completion_tokens.toLocaleString()} mono />
            <DetailRow label="总 Tokens" value={log.total_tokens.toLocaleString()} mono />
            <DetailRow label="延迟" value={`${log.duration_ms}ms`} mono />
          </div>
        </div>

        {/* 错误信息 */}
        {log.error_message && (
          <div>
            <h4 className="text-sm font-semibold text-text-primary mb-2">错误信息</h4>
            <div className="px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg">
              <p className="text-xs text-danger mono break-all">{log.error_message}</p>
            </div>
          </div>
        )}

        {/* 安全审计 */}
        <div>
          <h4 className="text-sm font-semibold text-text-primary mb-2">安全审计</h4>
          <div className="grid grid-cols-2 gap-x-6">
            <DetailRow
              label="风险等级"
              value={
                <Badge
                  variant={
                    log.risk_level === "high" || log.risk_level === "critical"
                      ? "danger"
                      : log.risk_level === "medium"
                        ? "warning"
                        : log.risk_level === "low"
                          ? "success"
                          : "neutral"
                  }
                >
                  {log.risk_level || "none"}
                </Badge>
              }
            />
            <DetailRow label="风险评分" value={log.risk_score} mono />
            <DetailRow label="安全动作" value={log.security_action || "none"} />
            <DetailRow label="已脱敏" value={log.sanitized ? "是" : "否"} />
            {log.blocked_reason && (
              <DetailRow label="拦截原因" value={log.blocked_reason} />
            )}
          </div>

          {/* 风险明细 */}
          {findings && findings.length > 0 && (
            <div className="mt-3">
              <p className="text-xs font-medium text-text-secondary mb-2">
                风险明细（{findings.length}）
              </p>
              <ul className="space-y-2">
                {findings.map((f: SecurityFinding) => (
                  <li
                    key={f.id}
                    className="flex items-start gap-2 px-3 py-2 bg-bg-tertiary rounded-lg border border-border-primary/50"
                  >
                    <Badge
                      variant={
                        f.severity === "critical" || f.severity === "high"
                          ? "danger"
                          : f.severity === "medium"
                            ? "warning"
                            : "neutral"
                      }
                    >
                      {f.severity}
                    </Badge>
                    <div className="flex-1 min-w-0">
                      <code className="text-xs mono text-text-primary break-all">
                        {f.rule}
                      </code>
                      {f.detail && (
                        <p className="text-xs text-text-muted mt-0.5 break-all">
                          {f.detail}
                        </p>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        {/* 请求体 */}
        {log.request_body && (
          <div>
            <h4 className="text-sm font-semibold text-text-primary mb-2">请求体</h4>
            <SyntaxHighlighter
              language="json"
              style={isDark ? oneDark : oneLight}
              customStyle={{
                margin: 0,
                borderRadius: "0.5rem",
                fontSize: "12px",
                background: "var(--bg-tertiary)",
              }}
              wrapLongLines
            >
              {prettyJson(log.request_body)}
            </SyntaxHighlighter>
          </div>
        )}

        {/* 响应选择（response_choices，多候选/流式聚合结果） */}
        {log.response_choices && (
          <div>
            <h4 className="text-sm font-semibold text-text-primary mb-2">响应选择</h4>
            <SyntaxHighlighter
              language="json"
              style={isDark ? oneDark : oneLight}
              customStyle={{
                margin: 0,
                borderRadius: "0.5rem",
                fontSize: "12px",
                background: "var(--bg-tertiary)",
              }}
              wrapLongLines
            >
              {prettyJson(log.response_choices)}
            </SyntaxHighlighter>
          </div>
        )}

        {/* 脱敏后转发体（实际发送到上游） */}
        {log.forward_body && (
          <div>
            <h4 className="text-sm font-semibold text-text-primary mb-2">
              脱敏后转发体（实际发送到上游）
            </h4>
            <SyntaxHighlighter
              language="json"
              style={isDark ? oneDark : oneLight}
              customStyle={{
                margin: 0,
                borderRadius: "0.5rem",
                fontSize: "12px",
                background: "var(--bg-tertiary)",
              }}
              wrapLongLines
            >
              {prettyJson(log.forward_body)}
            </SyntaxHighlighter>
          </div>
        )}
      </div>
    </Modal>
  );
}
