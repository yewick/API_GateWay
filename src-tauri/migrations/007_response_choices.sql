-- 为日志表增加 response_choices 字段（存储模型返回的 choices 内容，JSON 字符串）
ALTER TABLE request_logs ADD COLUMN response_choices TEXT;
