-- 对齐内置规则与 scanner 实现：
-- 1. 修正 b020 的 rule_id（network_or_exec → command）
-- 2. 移除两条语义复杂、scanner 尚未实现的幽灵规则
UPDATE security_builtin_rules SET rule_id = 'tool.shell.command' WHERE rule_id = 'tool.shell.network_or_exec';
DELETE FROM security_builtin_rules WHERE rule_id IN ('prompt.fingerprint_context', 'prompt.injection');
