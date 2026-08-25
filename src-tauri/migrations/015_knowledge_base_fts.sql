-- 知识库全文检索（FTS5）索引：为混合检索提供关键词搜索能力。
-- 同步 kb_chunks 的 content / symbol_name，使正文与代码符号名均可被关键词命中。

-- FTS 表：chunk_id 不参与分词索引（仅作回查外键），content / symbol_name 参与检索。
CREATE VIRTUAL TABLE IF NOT EXISTS kb_chunks_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    symbol_name,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- 新增 chunk → 同步 FTS（显式写入 rowid，令 FTS rowid 与 kb_chunks.rowid 对齐，
-- 删除/更新时即可用 'delete' 命令按 rowid 精确删除对应行）
CREATE TRIGGER IF NOT EXISTS kb_chunks_fts_ai AFTER INSERT ON kb_chunks BEGIN
    INSERT INTO kb_chunks_fts(rowid, chunk_id, content, symbol_name)
    VALUES (new.rowid, new.id, new.content, COALESCE(new.symbol_name, ''));
END;

-- 删除 chunk → 同步 FTS
CREATE TRIGGER IF NOT EXISTS kb_chunks_fts_ad AFTER DELETE ON kb_chunks BEGIN
    INSERT INTO kb_chunks_fts(kb_chunks_fts, rowid, chunk_id, content, symbol_name)
    VALUES ('delete', old.rowid, old.id, old.content, COALESCE(old.symbol_name, ''));
END;

-- 更新 chunk → 先删旧再插新
CREATE TRIGGER IF NOT EXISTS kb_chunks_fts_au AFTER UPDATE ON kb_chunks BEGIN
    INSERT INTO kb_chunks_fts(kb_chunks_fts, rowid, chunk_id, content, symbol_name)
    VALUES ('delete', old.rowid, old.id, old.content, COALESCE(old.symbol_name, ''));
    INSERT INTO kb_chunks_fts(rowid, chunk_id, content, symbol_name)
    VALUES (new.rowid, new.id, new.content, COALESCE(new.symbol_name, ''));
END;

-- 回填存量 chunk（直写 FTS 表、不经过 kb_chunks 触发器，避免重复插入）
INSERT INTO kb_chunks_fts(rowid, chunk_id, content, symbol_name)
SELECT rowid, id, content, COALESCE(symbol_name, '') FROM kb_chunks;
