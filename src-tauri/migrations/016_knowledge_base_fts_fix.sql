-- 修复 015：`kb_chunks_fts` 是「独立（standalone）FTS5 表」，其内容由触发器写入，
-- 而 FTS5 的 'delete' 特殊 INSERT 命令仅适用于「外部内容表 / 无内容表」，
-- 在独立表上使用会触发 "SQL logic error"。
-- 独立表的增删改同步应使用标准 DELETE ... WHERE rowid（rowid 已在插入时与 kb_chunks.rowid 对齐）。

DROP TRIGGER IF EXISTS kb_chunks_fts_ad;
DROP TRIGGER IF EXISTS kb_chunks_fts_au;

CREATE TRIGGER IF NOT EXISTS kb_chunks_fts_ad AFTER DELETE ON kb_chunks BEGIN
    DELETE FROM kb_chunks_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS kb_chunks_fts_au AFTER UPDATE ON kb_chunks BEGIN
    DELETE FROM kb_chunks_fts WHERE rowid = old.rowid;
    INSERT INTO kb_chunks_fts(rowid, chunk_id, content, symbol_name)
    VALUES (new.rowid, new.id, new.content, COALESCE(new.symbol_name, ''));
END;
