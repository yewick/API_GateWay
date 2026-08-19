-- 知识库生产级能力扩展：分块配置 + 来源信息 + 对话历史 + 导入源 + 索引元数据。

-- 知识库：新增分块/过滤配置 + 索引状态
ALTER TABLE kb_knowledge_bases ADD COLUMN chunk_size INTEGER NOT NULL DEFAULT 512;
ALTER TABLE kb_knowledge_bases ADD COLUMN chunk_overlap INTEGER NOT NULL DEFAULT 64;
ALTER TABLE kb_knowledge_bases ADD COLUMN excluded_dirs TEXT NOT NULL DEFAULT '';
ALTER TABLE kb_knowledge_bases ADD COLUMN excluded_files TEXT NOT NULL DEFAULT '';
ALTER TABLE kb_knowledge_bases ADD COLUMN included_files TEXT NOT NULL DEFAULT '';
ALTER TABLE kb_knowledge_bases ADD COLUMN embedding_dim INTEGER NOT NULL DEFAULT 0;
ALTER TABLE kb_knowledge_bases ADD COLUMN index_status TEXT NOT NULL DEFAULT 'none';

-- 文档：新增来源信息
ALTER TABLE kb_documents ADD COLUMN source_type TEXT NOT NULL DEFAULT 'upload';
ALTER TABLE kb_documents ADD COLUMN source_url TEXT;
ALTER TABLE kb_documents ADD COLUMN source_path TEXT;
ALTER TABLE kb_documents ADD COLUMN doc_meta TEXT NOT NULL DEFAULT '{}';

-- 对话历史表（RAG 问答的上下文）
CREATE TABLE IF NOT EXISTS kb_conversations (
    id           TEXT PRIMARY KEY,
    kb_id        TEXT NOT NULL,
    role         TEXT NOT NULL,      -- user / assistant
    content      TEXT NOT NULL,
    sources      TEXT,               -- JSON: 来源引用信息
    model        TEXT,
    tokens_used  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

-- 导入源记录表
CREATE TABLE IF NOT EXISTS kb_sources (
    id           TEXT PRIMARY KEY,
    kb_id        TEXT NOT NULL,
    source_type  TEXT NOT NULL,      -- git / url / local_dir
    source_url   TEXT,
    source_path  TEXT,
    branch       TEXT,
    status       TEXT NOT NULL DEFAULT 'pending',
    file_count   INTEGER NOT NULL DEFAULT 0,
    error        TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

-- 索引元数据表
CREATE TABLE IF NOT EXISTS kb_index_meta (
    kb_id        TEXT PRIMARY KEY,
    index_type   TEXT NOT NULL DEFAULT 'hnsw',
    embedding_dim INTEGER NOT NULL DEFAULT 0,
    chunk_count  INTEGER NOT NULL DEFAULT 0,
    index_path   TEXT,
    built_at     TEXT,
    status       TEXT NOT NULL DEFAULT 'none',
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);
