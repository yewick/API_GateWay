-- 安全审计扩展
-- 给 request_logs 补充 RequestLog 结构体所需的列

ALTER TABLE request_logs ADD COLUMN seq INTEGER;
ALTER TABLE request_logs ADD COLUMN request_body TEXT;
ALTER TABLE request_logs ADD COLUMN risk_level TEXT NOT NULL DEFAULT 'none';
ALTER TABLE request_logs ADD COLUMN risk_score INTEGER NOT NULL DEFAULT 0;
ALTER TABLE request_logs ADD COLUMN risk_summary TEXT;
ALTER TABLE request_logs ADD COLUMN security_action TEXT NOT NULL DEFAULT 'none';
ALTER TABLE request_logs ADD COLUMN sanitized INTEGER NOT NULL DEFAULT 0;
ALTER TABLE request_logs ADD COLUMN blocked_reason TEXT;

-- 安全风险明细表（每次扫描命中的规则）
CREATE TABLE IF NOT EXISTS security_findings (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    log_id     TEXT NOT NULL,
    rule       TEXT NOT NULL,
    severity   TEXT NOT NULL,
    detail     TEXT,
    action     TEXT NOT NULL DEFAULT 'audit',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_findings_log ON security_findings(log_id);
