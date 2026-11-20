import type { ReactNode } from "react";
import type { RequestLog, SecurityFinding } from "../../types";
import { Badge } from "../ui/Badge";
import { formatTime, statusColor } from "../../lib/constants";
import type { NormalizedLog } from "../../lib/logNormalize";

function DetailRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="flex items-start justify-between py-2 border-b border-border-primary/50 last:border-0">
      <span className="text-xs text-text-muted w-28 flex-shrink-0">{label}</span>
      <span className={`text-xs text-text-primary text-right break-all ${mono ? "mono" : ""}`}>
        {value ?? "-"}
      </span>
    </div>
  );
}

interface LogOverviewProps {
  log: RequestLog;
  findings?: SecurityFinding[];
  normalized: NormalizedLog;
}

export function LogOverview({ log, findings, normalized }: LogOverviewProps) {
  return (
    <div className="space-y-6">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-2">基本信息</h4>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6">
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
          <DetailRow
            label="响应来源"
            value={
              normalized.responseSource === "response_body"
                ? "完整响应体"
                : normalized.responseSource === "response_choices"
                  ? "旧日志 choices"
                  : "无响应内容"
            }
          />
        </div>
      </div>

      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-2">Token 与延迟</h4>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6">
          <DetailRow label="提示词 Tokens" value={log.prompt_tokens.toLocaleString()} mono />
          <DetailRow label="补全 Tokens" value={log.completion_tokens.toLocaleString()} mono />
          <DetailRow label="总 Tokens" value={log.total_tokens.toLocaleString()} mono />
          <DetailRow label="延迟" value={`${log.duration_ms}ms`} mono />
        </div>
      </div>

      {log.error_message && (
        <div>
          <h4 className="text-sm font-semibold text-text-primary mb-2">错误信息</h4>
          <div className="px-3 py-2 bg-danger/10 border border-danger/30 rounded-lg">
            <p className="text-xs text-danger mono break-all">{log.error_message}</p>
          </div>
        </div>
      )}

      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-2">安全审计</h4>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6">
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
          {log.risk_summary && <DetailRow label="风险摘要" value={log.risk_summary} />}
          {log.blocked_reason && <DetailRow label="拦截原因" value={log.blocked_reason} />}
        </div>

        {findings && findings.length > 0 && (
          <div className="mt-3">
            <p className="text-xs font-medium text-text-secondary mb-2">
              风险明细（{findings.length}）
            </p>
            <ul className="space-y-2">
              {findings.map((finding) => (
                <li
                  key={finding.id}
                  className="flex items-start gap-2 px-3 py-2 bg-bg-tertiary rounded-lg border border-border-primary/50"
                >
                  <Badge
                    variant={
                      finding.severity === "critical" || finding.severity === "high"
                        ? "danger"
                        : finding.severity === "medium"
                          ? "warning"
                          : "neutral"
                    }
                  >
                    {finding.severity}
                  </Badge>
                  <div className="flex-1 min-w-0">
                    <code className="text-xs mono text-text-primary break-all">
                      {finding.rule}
                    </code>
                    {finding.detail && (
                      <p className="text-xs text-text-muted mt-0.5 break-all">
                        {finding.detail}
                      </p>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
