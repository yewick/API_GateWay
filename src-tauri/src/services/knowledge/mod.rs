//! 知识库服务：创建私有知识库、上传文档自动解析分块落库，
//! 后续接入向量化、索引构建与 MCP 检索/RAG 问答工具。
//!
//! 本模块自包含数据模型 / 文档解析 / 文本分块 / Repository / HTTP 端点。

pub mod code_parser;
mod csv;
mod docx;
mod handlers;
mod html;
mod mineru;
pub mod models;
pub mod parser;
pub mod pdf;
mod pptx;
mod pymupdf;
pub mod repository;
pub mod splitter;
mod table;
mod xlsx;

use async_trait::async_trait;
use axum::routing::{delete, get};
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
                delete(handlers::delete_document),
            )
            .route(
                "/api/kb/{id}/documents/{doc_id}/content",
                get(handlers::get_document_content),
            )
            .route("/api/kb/{id}/stats", get(handlers::kb_stats))
            .route("/api/kb/status", get(handlers::kb_status))
    }
}
