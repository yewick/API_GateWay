-- 支持请求追踪标识（下游应用通过 x-trace-id header 注入自定义追踪 ID）
ALTER TABLE request_logs ADD COLUMN trace_id TEXT;
