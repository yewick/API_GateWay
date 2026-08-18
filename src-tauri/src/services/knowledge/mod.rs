//! 知识库服务：创建私有知识库，上传文档自动向量化并构建索引，
//! 通过 MCP 协议对外提供检索和 RAG 问答工具（当前为占位实现，具体逻辑后续补充）。

use async_trait::async_trait;
use axum::routing::get;
use axum::{Json, Router};
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
        // 查询真实统计（表缺失时兜底为 0，待知识库表落地后自动生效）
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
        // 占位路由：知识库 CRUD / 上传 / 检索 / 问答等具体路由后续补充
        Router::new()
            .route("/api/kb", get(list_knowledge_bases))
            .route("/api/kb/status", get(kb_status))
    }
}

/// GET /api/kb —— 返回知识库列表（表未建，先返回空列表）
async fn list_knowledge_bases() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": [] }))
}

async fn kb_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "knowledge",
        "name": "知识库",
        "registered": true,
        "note": "知识库功能待实现"
    }))
}
