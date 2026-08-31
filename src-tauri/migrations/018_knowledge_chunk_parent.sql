-- 父子块：子块（kb_chunks）仍是检索/向量/FTS 的最小单元，父块（kb_chunk_parents）仅用于
-- 在检索命中子块后补全 LLM 上下文。父块不参与向量/FTS 检索，按 parent_id 反查。

ALTER TABLE kb_chunks ADD COLUMN parent_id TEXT;

CREATE TABLE IF NOT EXISTS kb_chunk_parents (
    id           TEXT PRIMARY KEY,
    doc_id       TEXT NOT NULL,
    kb_id        TEXT NOT NULL,
    chunk_index  INTEGER NOT NULL,
    content      TEXT NOT NULL,
    token_count  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (doc_id) REFERENCES kb_documents(id) ON DELETE CASCADE,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chunk_parents_doc ON kb_chunk_parents(doc_id);
CREATE INDEX IF NOT EXISTS idx_chunk_parents_kb ON kb_chunk_parents(kb_id);
