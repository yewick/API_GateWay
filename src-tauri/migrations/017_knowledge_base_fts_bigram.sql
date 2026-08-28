-- 017: 停用 FTS5 触发器同步，改由应用层显式同步「CJK bigram」内容。
--
-- 原因：CJK bigram 分词在 Rust 侧计算（见 services/knowledge/tokenize.rs），
-- 触发器无法调用 Rust 分词，仍会把「原文」写入 kb_chunks_fts，导致索引内容
-- 与查询侧 bigram 不一致（中文子串无法召回）。因此删除三个触发器，
-- 由 KbRepository::{create_chunk, delete_chunks_by_doc, delete_kb} 显式同步 bigram 文本，
-- 并在启动时经 ensure_fts_bigram_index 全量重建一次（以 PRAGMA user_version 标记幂等）。

DROP TRIGGER IF EXISTS kb_chunks_fts_ai;
DROP TRIGGER IF EXISTS kb_chunks_fts_ad;
DROP TRIGGER IF EXISTS kb_chunks_fts_au;
