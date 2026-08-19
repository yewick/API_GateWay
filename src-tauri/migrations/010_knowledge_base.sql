-- 知识库核心四表：知识库 → 文档 → 切片 的层级关系，外加处理任务表。

-- 知识库：一个知识库对应一个主题/项目
CREATE TABLE IF NOT EXISTS kb_knowledge_bases (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    status        INTEGER NOT NULL DEFAULT 1,      -- 1=active, 0=disabled
    doc_count     INTEGER NOT NULL DEFAULT 0,
    chunk_count   INTEGER NOT NULL DEFAULT 0,
    total_tokens  INTEGER NOT NULL DEFAULT 0,
    embedding_model  TEXT,                          -- 向量模型名
    embedding_channel_id TEXT,                      -- 向量渠道 ID
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- 文档：上传的每个文件是一条记录
CREATE TABLE IF NOT EXISTS kb_documents (
    id            TEXT PRIMARY KEY,
    kb_id         TEXT NOT NULL,
    filename      TEXT NOT NULL,
    file_path     TEXT,                             -- 本地文件路径（如果是导入）
    file_type     TEXT NOT NULL,                    -- pdf/txt/md/docx/code...
    file_size     INTEGER NOT NULL DEFAULT 0,
    content_hash  TEXT NOT NULL,                    -- SHA256，防止重复上传
    chunk_count   INTEGER NOT NULL DEFAULT 0,
    token_count   INTEGER NOT NULL DEFAULT 0,
    status        TEXT NOT NULL DEFAULT 'pending',  -- pending/processing/ready/failed
    error_message TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

-- 切片：文档被分块后的最小检索单元
CREATE TABLE IF NOT EXISTS kb_chunks (
    id             TEXT PRIMARY KEY,
    doc_id         TEXT NOT NULL,
    kb_id          TEXT NOT NULL,
    chunk_index    INTEGER NOT NULL,                -- 分块序号
    content        TEXT NOT NULL,                   -- 分块文本内容
    token_count    INTEGER NOT NULL DEFAULT 0,
    embedding      BLOB,                            -- 向量数据（二进制）
    embedding_dim  INTEGER NOT NULL DEFAULT 0,      -- 向量维度
    metadata       TEXT NOT NULL DEFAULT '{}',      -- JSON元数据
    created_at     TEXT NOT NULL,
    FOREIGN KEY (doc_id) REFERENCES kb_documents(id) ON DELETE CASCADE,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

-- 处理任务：记录异步处理进度
CREATE TABLE IF NOT EXISTS kb_tasks (
    id            TEXT PRIMARY KEY,
    kb_id         TEXT NOT NULL,
    doc_id        TEXT,
    task_type     TEXT NOT NULL,                    -- embed/index/import
    status        TEXT NOT NULL DEFAULT 'pending',
    progress      INTEGER NOT NULL DEFAULT 0,
    total_items   INTEGER NOT NULL DEFAULT 0,
    done_items    INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    completed_at  TEXT,
    FOREIGN KEY (kb_id) REFERENCES kb_knowledge_bases(id) ON DELETE CASCADE
);

-- 外键列索引：加速按知识库/文档查切片
CREATE INDEX IF NOT EXISTS idx_documents_kb ON kb_documents(kb_id);
CREATE INDEX IF NOT EXISTS idx_chunks_doc ON kb_chunks(doc_id);
CREATE INDEX IF NOT EXISTS idx_chunks_kb ON kb_chunks(kb_id);
