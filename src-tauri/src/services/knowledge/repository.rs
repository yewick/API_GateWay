//! 知识库 Repository：`kb_*` 各表的 CRUD 数据访问层。
//!
//! 提供完整契约；对话、导入源、向量、单条查询等部分方法对应的 HTTP/命令流程
//! 尚未接入，暂以 `#![allow(dead_code)]` 抑制未使用告警，待后续流程接入后移除。

#![allow(dead_code)]

use sqlx::SqlitePool;

use super::models::*;

pub struct KbRepository {
    pool: SqlitePool,
}

impl KbRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // -------------------------------------------------------------------------
    // Knowledge Base CRUD
    // -------------------------------------------------------------------------

    pub async fn get_all_kbs(&self) -> Result<Vec<KbKnowledgeBase>, sqlx::Error> {
        sqlx::query_as::<_, KbKnowledgeBase>(
            "SELECT * FROM kb_knowledge_bases ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_kb(&self, id: &str) -> Result<KbKnowledgeBase, sqlx::Error> {
        sqlx::query_as::<_, KbKnowledgeBase>("SELECT * FROM kb_knowledge_bases WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn create_kb(&self, input: &CreateKbInput) -> Result<KbKnowledgeBase, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = crate::utils::time::now_iso();
        sqlx::query(
            "INSERT INTO kb_knowledge_bases \
             (id, name, description, embedding_model, embedding_channel_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.embedding_model)
        .bind(&input.embedding_channel_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        self.get_kb(&id).await
    }

    pub async fn update_kb(
        &self,
        id: &str,
        input: &UpdateKbInput,
    ) -> Result<KbKnowledgeBase, sqlx::Error> {
        let now = crate::utils::time::now_iso();
        let mut q = sqlx::QueryBuilder::new("UPDATE kb_knowledge_bases SET updated_at = ");
        q.push_bind(&now);

        if let Some(v) = &input.name {
            q.push(", name = ").push_bind(v);
        }
        if let Some(v) = &input.description {
            q.push(", description = ").push_bind(v);
        }
        if let Some(v) = &input.embedding_model {
            q.push(", embedding_model = ").push_bind(v);
        }
        if let Some(v) = &input.embedding_channel_id {
            q.push(", embedding_channel_id = ").push_bind(v);
        }
        if let Some(v) = input.status {
            q.push(", status = ").push_bind(v);
        }
        if let Some(v) = input.mcp_enabled {
            q.push(", mcp_enabled = ").push_bind(v);
        }
        if let Some(v) = input.chunk_size {
            q.push(", chunk_size = ").push_bind(v);
        }
        if let Some(v) = input.chunk_overlap {
            q.push(", chunk_overlap = ").push_bind(v);
        }
        if let Some(v) = &input.excluded_dirs {
            q.push(", excluded_dirs = ").push_bind(v);
        }
        if let Some(v) = &input.excluded_files {
            q.push(", excluded_files = ").push_bind(v);
        }
        if let Some(v) = &input.included_files {
            q.push(", included_files = ").push_bind(v);
        }

        q.push(" WHERE id = ").push_bind(id);
        q.build().execute(&self.pool).await?;
        self.get_kb(id).await
    }

    pub async fn delete_kb(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kb_knowledge_bases WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Document CRUD
    // -------------------------------------------------------------------------

    pub async fn get_documents(&self, kb_id: &str) -> Result<Vec<KbDocument>, sqlx::Error> {
        sqlx::query_as::<_, KbDocument>(
            "SELECT * FROM kb_documents WHERE kb_id = ? ORDER BY created_at DESC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_document(&self, doc_id: &str) -> Result<KbDocument, sqlx::Error> {
        sqlx::query_as::<_, KbDocument>("SELECT * FROM kb_documents WHERE id = ?")
            .bind(doc_id)
            .fetch_one(&self.pool)
            .await
    }

    /// 按内容哈希查重（同一知识库内相同文件跳过）
    pub async fn get_document_by_hash(
        &self,
        kb_id: &str,
        hash: &str,
    ) -> Result<Option<KbDocument>, sqlx::Error> {
        sqlx::query_as::<_, KbDocument>(
            "SELECT * FROM kb_documents WHERE kb_id = ? AND content_hash = ?",
        )
        .bind(kb_id)
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create_document(&self, doc: &KbDocument) -> Result<KbDocument, sqlx::Error> {
        sqlx::query(
            "INSERT INTO kb_documents \
             (id, kb_id, filename, file_path, file_type, file_size, content_hash, content, \
              chunk_count, token_count, status, error_message, source_type, source_url, \
              source_path, doc_meta, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&doc.id)
        .bind(&doc.kb_id)
        .bind(&doc.filename)
        .bind(&doc.file_path)
        .bind(&doc.file_type)
        .bind(doc.file_size)
        .bind(&doc.content_hash)
        .bind(&doc.content)
        .bind(doc.chunk_count)
        .bind(doc.token_count)
        .bind(&doc.status)
        .bind(&doc.error_message)
        .bind(&doc.source_type)
        .bind(&doc.source_url)
        .bind(&doc.source_path)
        .bind(&doc.doc_meta)
        .bind(&doc.created_at)
        .bind(&doc.updated_at)
        .execute(&self.pool)
        .await?;
        self.get_document(&doc.id).await
    }

    pub async fn update_document_status(
        &self,
        doc_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE kb_documents SET status = ?, error_message = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(error_message)
            .bind(crate::utils::time::now_iso())
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 回写解析结果（content + file_type），并同时切状态（`awaiting_review`）。
    pub async fn update_document_content(
        &self,
        doc_id: &str,
        content: &str,
        file_type: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_documents SET content = ?, file_type = ?, status = ?, \
             error_message = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(content)
        .bind(file_type)
        .bind(status)
        .bind(crate::utils::time::now_iso())
        .bind(doc_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 回写分块后的 chunk 统计（chunk_count + token_count）。
    pub async fn update_document_chunk_stats(
        &self,
        doc_id: &str,
        chunk_count: i64,
        token_count: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_documents SET chunk_count = ?, token_count = ?, updated_at = ? WHERE id = ?",
        )
        .bind(chunk_count)
        .bind(token_count)
        .bind(crate::utils::time::now_iso())
        .bind(doc_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_document(&self, doc_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kb_documents WHERE id = ?")
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Task CRUD（kb_tasks：异步处理进度）
    // -------------------------------------------------------------------------

    pub async fn create_task(&self, task: &KbTask) -> Result<KbTask, sqlx::Error> {
        sqlx::query(
            "INSERT INTO kb_tasks \
             (id, kb_id, doc_id, task_type, status, progress, total_items, done_items, \
              error_message, created_at, completed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&task.id)
        .bind(&task.kb_id)
        .bind(&task.doc_id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(task.progress)
        .bind(task.total_items)
        .bind(task.done_items)
        .bind(&task.error_message)
        .bind(&task.created_at)
        .bind(&task.completed_at)
        .execute(&self.pool)
        .await?;
        Ok(task.clone())
    }

    /// 更新进度（progress = 百分比 0~100；done/total 为原始单位，total=0 表示未知）。
    pub async fn update_task_progress(
        &self,
        task_id: &str,
        progress: i64,
        done_items: i64,
        total_items: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_tasks SET progress = ?, done_items = ?, total_items = ? WHERE id = ?",
        )
        .bind(progress)
        .bind(done_items)
        .bind(total_items)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn complete_task(&self, task_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_tasks SET status = 'done', progress = 100, completed_at = ? WHERE id = ?",
        )
        .bind(crate::utils::time::now_iso())
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_tasks SET status = 'failed', error_message = ?, completed_at = ? WHERE id = ?",
        )
        .bind(error)
        .bind(crate::utils::time::now_iso())
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_task(&self, task_id: &str) -> Result<KbTask, sqlx::Error> {
        sqlx::query_as::<_, KbTask>("SELECT * FROM kb_tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&self.pool)
            .await
    }

    /// 取某文档最新的任务（按创建时间倒序）。
    pub async fn get_latest_task_by_doc(&self, doc_id: &str) -> Result<Option<KbTask>, sqlx::Error> {
        sqlx::query_as::<_, KbTask>(
            "SELECT * FROM kb_tasks WHERE doc_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_tasks_by_kb(&self, kb_id: &str) -> Result<Vec<KbTask>, sqlx::Error> {
        sqlx::query_as::<_, KbTask>(
            "SELECT * FROM kb_tasks WHERE kb_id = ? ORDER BY created_at DESC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await
    }

    // -------------------------------------------------------------------------
    // Chunk CRUD
    // -------------------------------------------------------------------------

    pub async fn get_chunks_by_kb(&self, kb_id: &str) -> Result<Vec<KbChunk>, sqlx::Error> {
        sqlx::query_as::<_, KbChunk>(
            "SELECT * FROM kb_chunks WHERE kb_id = ? ORDER BY chunk_index ASC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn create_chunk(&self, chunk: &KbChunk) -> Result<KbChunk, sqlx::Error> {
        sqlx::query(
            "INSERT INTO kb_chunks \
             (id, doc_id, kb_id, chunk_index, content, token_count, embedding, \
              embedding_dim, metadata, symbol_name, symbol_kind, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&chunk.id)
        .bind(&chunk.doc_id)
        .bind(&chunk.kb_id)
        .bind(chunk.chunk_index)
        .bind(&chunk.content)
        .bind(chunk.token_count)
        .bind(&chunk.embedding)
        .bind(chunk.embedding_dim)
        .bind(&chunk.metadata)
        .bind(&chunk.symbol_name)
        .bind(&chunk.symbol_kind)
        .bind(&chunk.created_at)
        .execute(&self.pool)
        .await?;
        Ok(chunk.clone())
    }

    /// 批量插入切片（上传文档时使用）
    pub async fn insert_chunks_bulk(&self, chunks: &[KbChunk]) -> Result<(), sqlx::Error> {
        for c in chunks {
            self.create_chunk(c).await?;
        }
        Ok(())
    }

    pub async fn delete_chunks_by_doc(&self, doc_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kb_chunks WHERE doc_id = ?")
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 更新切片向量（f32 小端字节序序列化，暂不引入 bincode）
    pub async fn update_chunk_embedding(
        &self,
        chunk_id: &str,
        embedding: &[f32],
    ) -> Result<(), sqlx::Error> {
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query("UPDATE kb_chunks SET embedding = ?, embedding_dim = ? WHERE id = ?")
            .bind(bytes)
            .bind(embedding.len() as i64)
            .bind(chunk_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 关键词搜索（FTS5）：返回按 bm25 排名升序的命中。
    /// 查询串先做引号转义，避免注入 FTS5 查询语法。
    pub async fn search_fts(
        &self,
        kb_id: &str,
        query: &str,
        top_k: i64,
    ) -> Result<Vec<FtsHit>, sqlx::Error> {
        let query = query.trim().replace('"', "\"\"");
        sqlx::query_as::<_, FtsHit>(
            "SELECT c.id AS chunk_id, c.doc_id, c.content, bm25(kb_chunks_fts) AS rank \
             FROM kb_chunks_fts JOIN kb_chunks c ON c.id = kb_chunks_fts.chunk_id \
             WHERE kb_chunks_fts MATCH ? AND c.kb_id = ? ORDER BY rank LIMIT ?",
        )
        .bind(query)
        .bind(kb_id)
        .bind(top_k)
        .fetch_all(&self.pool)
        .await
    }

    /// 取知识库内已向量化的 chunk（按确定性顺序：created_at, id），返回
    /// `(chunk_id, 向量)`，供索引构建使用。
    pub async fn get_chunks_with_embeddings(
        &self,
        kb_id: &str,
    ) -> Result<Vec<(String, Vec<f32>)>, sqlx::Error> {
        let rows: Vec<(String, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT id, embedding FROM kb_chunks \
             WHERE kb_id = ? AND embedding IS NOT NULL \
             ORDER BY created_at ASC, id ASC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, emb)| emb.and_then(|b| decode_embedding(&b)).map(|v| (id, v)))
            .collect())
    }

    // -------------------------------------------------------------------------
    // Conversation History
    // -------------------------------------------------------------------------

    pub async fn add_conversation(
        &self,
        kb_id: &str,
        role: &str,
        content: &str,
        sources: Option<&str>,
        model: Option<&str>,
        tokens_used: i64,
    ) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO kb_conversations \
             (id, kb_id, role, content, sources, model, tokens_used, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(kb_id)
        .bind(role)
        .bind(content)
        .bind(sources)
        .bind(model)
        .bind(tokens_used)
        .bind(crate::utils::time::now_iso())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_conversations(&self, kb_id: &str) -> Result<Vec<KbConversation>, sqlx::Error> {
        sqlx::query_as::<_, KbConversation>(
            "SELECT * FROM kb_conversations WHERE kb_id = ? ORDER BY created_at ASC",
        )
        .bind(kb_id)
        .fetch_all(&self.pool)
        .await
    }

    // -------------------------------------------------------------------------
    // Sources
    // -------------------------------------------------------------------------

    pub async fn create_source(&self, source: &KbSource) -> Result<KbSource, sqlx::Error> {
        sqlx::query(
            "INSERT INTO kb_sources \
             (id, kb_id, source_type, source_url, source_path, branch, status, \
              file_count, error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&source.id)
        .bind(&source.kb_id)
        .bind(&source.source_type)
        .bind(&source.source_url)
        .bind(&source.source_path)
        .bind(&source.branch)
        .bind(&source.status)
        .bind(source.file_count)
        .bind(&source.error)
        .bind(&source.created_at)
        .bind(&source.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(source.clone())
    }

    pub async fn list_sources(&self, kb_id: &str) -> Result<Vec<KbSource>, sqlx::Error> {
        sqlx::query_as::<_, KbSource>("SELECT * FROM kb_sources WHERE kb_id = ? ORDER BY created_at DESC")
            .bind(kb_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn delete_source(&self, source_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kb_sources WHERE id = ?")
            .bind(source_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // 索引元数据
    // -------------------------------------------------------------------------

    pub async fn get_index_meta(&self, kb_id: &str) -> Result<Option<KbIndexMeta>, sqlx::Error> {
        sqlx::query_as::<_, KbIndexMeta>("SELECT * FROM kb_index_meta WHERE kb_id = ?")
            .bind(kb_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// 插入或更新索引元数据（kb_id 为主键冲突时覆盖）。
    pub async fn upsert_index_meta(&self, meta: &KbIndexMeta) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO kb_index_meta \
             (kb_id, index_type, embedding_dim, chunk_count, index_path, built_at, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(kb_id) DO UPDATE SET \
                 index_type = excluded.index_type, \
                 embedding_dim = excluded.embedding_dim, \
                 chunk_count = excluded.chunk_count, \
                 index_path = excluded.index_path, \
                 built_at = excluded.built_at, \
                 status = excluded.status",
        )
        .bind(&meta.kb_id)
        .bind(&meta.index_type)
        .bind(meta.embedding_dim)
        .bind(meta.chunk_count)
        .bind(&meta.index_path)
        .bind(&meta.built_at)
        .bind(&meta.status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 更新知识库索引状态（`none` / `building` / `ready` / `failed`）。
    pub async fn update_index_status(&self, kb_id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE kb_knowledge_bases SET index_status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(crate::utils::time::now_iso())
            .bind(kb_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // 统计
    // -------------------------------------------------------------------------

    pub async fn get_kb_stats(&self, kb_id: &str) -> Result<KbStats, sqlx::Error> {
        sqlx::query_as::<_, KbStats>(
            "SELECT doc_count, chunk_count, total_tokens FROM kb_knowledge_bases WHERE id = ?",
        )
        .bind(kb_id)
        .fetch_one(&self.pool)
        .await
    }

    /// 上传/删除文档后增量更新知识库计数
    pub async fn increment_kb_counts(
        &self,
        kb_id: &str,
        doc_delta: i64,
        chunk_delta: i64,
        token_delta: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE kb_knowledge_bases \
             SET doc_count = doc_count + ?, chunk_count = chunk_count + ?, \
                 total_tokens = total_tokens + ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(doc_delta)
        .bind(chunk_delta)
        .bind(token_delta)
        .bind(crate::utils::time::now_iso())
        .bind(kb_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// 解码 embedding BLOB（f32 小端字节序）为 `Vec<f32>`。
fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(out)
}
