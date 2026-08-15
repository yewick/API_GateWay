-- 记录实际转发给上游的请求体（脱敏后），便于在日志详情观察脱敏结果
ALTER TABLE request_logs ADD COLUMN forward_body TEXT;
