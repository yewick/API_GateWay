-- 为 chunk 新增符号信息（来自 tree-sitter AST 解析）
ALTER TABLE kb_chunks ADD COLUMN symbol_name TEXT;
ALTER TABLE kb_chunks ADD COLUMN symbol_kind TEXT;

-- 符号过滤索引（部分索引：只索引有 symbol_name 的行）
CREATE INDEX IF NOT EXISTS idx_chunks_symbol ON kb_chunks(kb_id, symbol_kind)
    WHERE symbol_name IS NOT NULL;
