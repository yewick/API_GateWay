-- 内置安全规则（应用播种，用户可编辑）
CREATE TABLE IF NOT EXISTS security_builtin_rules (
    id          TEXT PRIMARY KEY,
    rule_id     TEXT NOT NULL UNIQUE,   -- 如 credential.secret_token
    category    TEXT NOT NULL,          -- credential | file | unicode | network | tool | prompt
    severity    TEXT NOT NULL DEFAULT 'medium',
    title       TEXT NOT NULL,
    description TEXT,
    toggle_key  TEXT,                   -- 关联的设置开关，如 security.scan_unicode
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

-- 自定义规则（用户黑白名单）
CREATE TABLE IF NOT EXISTS security_custom_rules (
    id          TEXT PRIMARY KEY,
    rule_type   TEXT NOT NULL,          -- blacklist | whitelist
    category    TEXT NOT NULL,          -- domain | tool | path | keyword
    pattern     TEXT NOT NULL,          -- 匹配内容（子串匹配）
    severity    TEXT NOT NULL DEFAULT 'medium',
    action      TEXT NOT NULL DEFAULT 'warn',
    enabled     INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_builtin_rules_enabled ON security_builtin_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_custom_rules_type ON security_custom_rules(rule_type);
-- ... 其余索引