//! MCP Server 服务：Model Context Protocol Server，对外暴露知识库工具。

mod handlers;

use async_trait::async_trait;
use axum::routing::{get, post};
use axum::{Json, Router};
use std::sync::Arc;
use crate::AppState;
use crate::server::router::SharedState;
use super::{Service, ServiceStatus};

pub struct McpService;

#[async_trait]
impl Service for McpService {
    fn id(&self) -> &'static str {
        "mcp"
    }

    fn name(&self) -> &'static str {
        "MCP Server"
    }

    fn description(&self) -> &'static str {
        "Model Context Protocol Server，对外暴露知识库工具（支持创建/更新/删除知识库、\
         上传/删除文档、导入源、构建索引、搜索、RAG问答）"
    }

    async fn status(&self, state: &Arc<AppState>) -> ServiceStatus {
        let pool = &state.db.pool;
        let kb_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM kb_knowledge_bases WHERE status = 1",
        )
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
                "available_knowledge_bases": kb_count,
                "tools": handlers::mcp_tools(),
            }),
        }
    }

    fn routes(&self, _state: Arc<AppState>) -> Router<SharedState> {
        Router::new()
            // Streamable HTTP 端点（JSON-RPC）
            .route("/mcp", post(handlers::handle_mcp))
            // 传统 SSE 握手端点
            .route("/mcp/sse", get(handlers::handle_mcp_sse))
            .route("/mcp/health", get(mcp_health))
    }
}

async fn mcp_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "mcp",
        "name": "MCP Server",
        "registered": true,
        "note": "MCP 协议处理待实现"
    }))
}
