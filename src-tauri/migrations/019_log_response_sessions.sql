-- 日志详情与会话阅读：最终响应体 + 会话归并字段
ALTER TABLE request_logs ADD COLUMN response_body TEXT;
ALTER TABLE request_logs ADD COLUMN session_key TEXT;
ALTER TABLE request_logs ADD COLUMN client_session_id TEXT;
ALTER TABLE request_logs ADD COLUMN session_source TEXT NOT NULL DEFAULT 'none';
ALTER TABLE request_logs ADD COLUMN session_client TEXT;
ALTER TABLE request_logs ADD COLUMN session_title TEXT;
ALTER TABLE request_logs ADD COLUMN request_kind TEXT NOT NULL DEFAULT 'turn';
ALTER TABLE request_logs ADD COLUMN history_len INTEGER;

CREATE INDEX IF NOT EXISTS idx_logs_session_seq ON request_logs(session_key, seq);
CREATE INDEX IF NOT EXISTS idx_logs_session_time ON request_logs(session_key, created_at);
