//! 知识库服务：创建私有知识库、上传文档自动解析分块落库，
//! 后续接入向量化、索引构建与 MCP 检索/RAG 问答工具。
//!
//! 本模块自包含数据模型 / 文档解析 / 文本分块 / Repository / HTTP 端点。

pub mod code_parser;
mod csv;
mod docx;
mod embedder;
mod handlers;
mod html;
mod importer;
mod index;
mod mineru;
pub mod models;
pub mod parser;
pub mod pdf;
mod pptx;
mod processor;
mod pymupdf;
pub mod rag;
pub mod repository;
pub mod retriever;
pub mod splitter;
mod table;
mod tokenize;
mod xlsx;

pub use embedder::{embed, validate_embedding_config};
pub use importer::{import_source, ImportSummary};
pub use processor::{
    build_index, get_index_status, ingest_document, parse_document_background, process_document,
    ProcessOutcome, SourceInfo,
};

/// 一次性把 FTS5 索引从「原文」重建为「CJK bigram」内容（幂等）。
/// 用 `PRAGMA user_version` 作迁移标记：`< 1` 表示尚未重建，重建成功后置为 `1`。
/// 由 `lib.rs::run()` 在数据库迁移完成后调用一次。
pub async fn ensure_fts_bigram_index(pool: &sqlx::SqlitePool) {
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if version >= 1 {
        return;
    }
    let repo = repository::KbRepository::new(pool.clone());
    if let Err(e) = repo.rebuild_fts().await {
        tracing::warn!("FTS bigram 索引重建失败: {e}");
        return;
    }
    let _ = sqlx::query("PRAGMA user_version = 1").execute(pool).await;
}

use async_trait::async_trait;
use axum::routing::{delete, get, post};
use axum::Router;
use std::sync::Arc;
use crate::AppState;
use crate::server::router::SharedState;
use super::{Service, ServiceStatus};

pub struct KnowledgeService;

#[async_trait]
impl Service for KnowledgeService {
    fn id(&self) -> &'static str {
        "knowledge"
    }

    fn name(&self) -> &'static str {
        "知识库"
    }

    fn description(&self) -> &'static str {
        "本地知识库：创建私有知识库，上传文档自动向量化并构建 HNSW 索引，\
         通过 MCP 协议对外提供检索和 RAG 问答工具，支持任意 AI Agent 对接"
    }

    async fn status(&self, state: &Arc<AppState>) -> ServiceStatus {
        let pool = &state.db.pool;
        let kb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_knowledge_bases")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_documents")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_chunks")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        ServiceStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            enabled: true,
            running: true,
            stats: serde_json::json!({
                "knowledge_bases": kb_count,
                "documents": doc_count,
                "chunks": chunk_count,
            }),
        }
    }

    fn routes(&self, _state: Arc<AppState>) -> Router<SharedState> {
        Router::new()
            .route(
                "/api/kb",
                get(handlers::list_knowledge_bases).post(handlers::create_knowledge_base),
            )
            .route(
                "/api/kb/{id}",
                get(handlers::get_knowledge_base)
                    .put(handlers::update_knowledge_base)
                    .delete(handlers::delete_knowledge_base),
            )
            .route(
                "/api/kb/{id}/documents",
                get(handlers::list_documents).post(handlers::upload_document),
            )
            .route(
                "/api/kb/{id}/documents/{doc_id}",
                get(handlers::get_document).delete(handlers::delete_document),
            )
            .route(
                "/api/kb/{id}/documents/{doc_id}/content",
                get(handlers::get_document_content),
            )
            .route(
                "/api/kb/{id}/documents/{doc_id}/ingest",
                post(handlers::ingest_document),
            )
            .route("/api/kb/{id}/stats", get(handlers::kb_stats))
            .route("/api/kb/status", get(handlers::kb_status))
            .route(
                "/api/kb/{id}/index",
                get(handlers::get_index).post(handlers::build_index),
            )
            .route("/api/kb/{id}/search", get(handlers::search_fts))
            .route(
                "/api/kb/search",
                get(handlers::search),
            )
            .route(
                "/api/kb/ask",
                post(handlers::ask),
            )
            .route(
                "/api/kb/{id}/conversations",
                get(handlers::list_conversations),
            )
            .route(
                "/api/kb/{id}/sources",
                get(handlers::list_sources).post(handlers::import_source),
            )
            .route(
                "/api/kb/{id}/sources/{source_id}",
                delete(handlers::delete_source),
            )
    }
}
