//! 微服务式路由架构：借鉴 Service Registry 模式，适配本地单进程场景。
//!
//! 每个服务（知识库 / MCP）自包含：路由定义、状态检查、启用逻辑都在服务内部。
//! 新增服务只需实现 [`Service`] trait 并注册到 [`ServiceRegistry`]，
//! `server/router.rs` 通过 [`ServiceRegistry::merge_routes`] 一行合并所有服务路由。

pub mod knowledge;
pub mod mcp;

use async_trait::async_trait;
use axum::Router;
use serde::Serialize;
use std::sync::Arc;
use crate::AppState;
use crate::server::router::SharedState;

/// 服务状态（`stats` 用 JSON Value 兼容不同服务各异的统计结构）
#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub running: bool,
    pub stats: serde_json::Value,
}

/// Service trait —— 所有服务实现此接口
#[async_trait]
pub trait Service: Send + Sync {
    /// 服务唯一 id（如 "knowledge"、"mcp"）
    fn id(&self) -> &'static str;
    /// 展示名（如 "知识库"、"MCP Server"）
    fn name(&self) -> &'static str;
    /// 供 UI 展示的描述
    fn description(&self) -> &'static str;
    /// 是否启用（默认 true，未来可扩展为从配置读取）
    fn enabled(&self) -> bool {
        true
    }
    /// 服务状态（异步，可查数据库）
    async fn status(&self, state: &Arc<AppState>) -> ServiceStatus;
    /// 把本服务的路由注册进一个子 Router
    fn routes(&self, state: Arc<AppState>) -> Router<SharedState>;
}

/// 服务注册表：持有全部服务实例，负责路由合并与状态汇总
pub struct ServiceRegistry {
    services: Vec<Box<dyn Service>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        let mut registry = Self { services: vec![] };
        // 注册内置服务
        registry.register(Box::new(knowledge::KnowledgeService));
        registry.register(Box::new(mcp::McpService));
        registry
    }

    pub fn register(&mut self, service: Box<dyn Service>) {
        self.services.push(service);
    }

    /// 合并所有已启用服务的路由（各服务前缀 /api/kb、/mcp 与核心 /v1、/health 不冲突）
    pub fn merge_routes(&self, state: Arc<AppState>) -> Router<SharedState> {
        let mut router: Router<SharedState> = Router::new();
        for service in &self.services {
            if service.enabled() {
                router = router.merge(service.routes(state.clone()));
            }
        }
        router
    }

    /// 汇总所有服务状态（供前端统一展示）
    pub async fn list_status(&self, state: &Arc<AppState>) -> Vec<ServiceStatus> {
        let mut result = Vec::with_capacity(self.services.len());
        for service in &self.services {
            result.push(service.status(state).await);
        }
        result
    }
}
