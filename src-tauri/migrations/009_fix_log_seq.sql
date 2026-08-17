-- 修复 seq 字段的序列生成逻辑：
-- seq 由 002_security_audit.sql 引入（INTEGER，可空），历史行均为 NULL。
-- 用 rowid 回填，保证已有日志具备连续序列号；新日志由 create_log 在插入时生成。
UPDATE request_logs SET seq = rowid WHERE seq IS NULL;
