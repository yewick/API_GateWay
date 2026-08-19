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
             (id, kb_id, filename, file_path, file_type, file_size, content_hash, \
              chunk_count, token_count, status, error_message, source_type, source_url, \
              source_path, doc_meta, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&doc.id)
        .bind(&doc.kb_id)
        .bind(&doc.filename)
        .bind(&doc.file_path)
        .bind(&doc.file_type)
        .bind(doc.file_size)
        .bind(&doc.content_hash)
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

    pub async fn delete_document(&self, doc_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kb_documents WHERE id = ?")
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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
