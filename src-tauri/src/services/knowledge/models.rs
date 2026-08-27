//! 知识库数据模型：数据库表映射（`sqlx::FromRow`）+ 输入/输出 DTO。
//!
//! 完整模型契约一次性定义；部分模型（对话、导入源、索引元数据、RAG 问答）对应的
//! 处理流程尚未接入，暂以 `#![allow(dead_code)]` 抑制未使用告警。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 数据库表映射（对应 migrations 010~013 的最终 schema）
// ---------------------------------------------------------------------------

/// 知识库（`kb_knowledge_bases`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbKnowledgeBase {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: i64,
    pub doc_count: i64,
    pub chunk_count: i64,
    pub total_tokens: i64,
    pub embedding_model: Option<String>,
    pub embedding_channel_id: Option<String>,
    pub mcp_enabled: i64,
    pub chunk_size: i64,
    pub chunk_overlap: i64,
    pub excluded_dirs: String,
    pub excluded_files: String,
    pub included_files: String,
    pub embedding_dim: i64,
    pub index_status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 文档（`kb_documents`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbDocument {
    pub id: String,
    pub kb_id: String,
    pub filename: String,
    pub file_path: Option<String>,
    pub file_type: String,
    pub file_size: i64,
    pub content_hash: String,
    /// 解析后的完整文本（不切块；md/pdf → Markdown，txt → 纯文本，代码 → 源码）
    pub content: String,
    pub chunk_count: i64,
    pub token_count: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_path: Option<String>,
    pub doc_meta: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 切片（`kb_chunks`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbChunk {
    pub id: String,
    pub doc_id: String,
    pub kb_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub token_count: i64,
    /// 向量二进制（f32 小端字节序）；向量化流程接入前恒为 `None`
    pub embedding: Option<Vec<u8>>,
    pub embedding_dim: i64,
    pub metadata: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub created_at: String,
}

/// 处理任务（`kb_tasks`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbTask {
    pub id: String,
    pub kb_id: String,
    pub doc_id: Option<String>,
    pub task_type: String,
    pub status: String,
    pub progress: i64,
    pub total_items: i64,
    pub done_items: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// 对话历史（`kb_conversations`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbConversation {
    pub id: String,
    pub kb_id: String,
    pub role: String,
    pub content: String,
    pub sources: Option<String>,
    pub model: Option<String>,
    pub tokens_used: i64,
    pub created_at: String,
}

/// 导入源（`kb_sources`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbSource {
    pub id: String,
    pub kb_id: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_path: Option<String>,
    pub branch: Option<String>,
    pub status: String,
    pub file_count: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 索引元数据（`kb_index_meta`）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbIndexMeta {
    pub kb_id: String,
    pub index_type: String,
    pub embedding_dim: i64,
    pub chunk_count: i64,
    pub index_path: Option<String>,
    pub built_at: Option<String>,
    pub status: String,
}

// ---------------------------------------------------------------------------
// 输入 / 输出 DTO
// ---------------------------------------------------------------------------

/// 创建知识库输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKbInput {
    pub name: String,
    pub description: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_channel_id: Option<String>,
}

/// 更新知识库输入（所有字段可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateKbInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_channel_id: Option<String>,
    pub status: Option<i64>,
    pub mcp_enabled: Option<i64>,
    pub chunk_size: Option<i64>,
    pub chunk_overlap: Option<i64>,
    pub excluded_dirs: Option<String>,
    pub excluded_files: Option<String>,
    pub included_files: Option<String>,
}

/// 上传文档输入（`content` 为 base64 编码的原始文件字节）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadDocumentInput {
    pub filename: String,
    pub content: String,
}

/// RAG 问答输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskInput {
    pub question: String,
    pub kb_id: Option<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_model")]
    pub model: String,
    pub history: Option<Vec<ConversationMessage>>,
    #[serde(default)]
    pub deep_research: bool,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: usize,
    /// 请求级上下文上限覆盖（token 数，>0 时优先于渠道/模型配置）
    #[serde(default)]
    pub context_limit: Option<u64>,
}

/// 会话消息（`AskInput.history` 的组成单元）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

/// RAG 问答结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagAnswer {
    pub answer: String,
    pub sources: Vec<SearchResult>,
    pub usage: Option<RagUsage>,
}

/// RAG 问答 Token 用量（自含，避免依赖 `adaptor::TokenUsage`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// 文档解析内容（供前端查看器）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentContent {
    pub content: String,
    pub file_type: String,
}

/// 文档上传结果（Tauri 命令返回；`duplicate` 表示同内容已在库中）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadDocumentResult {
    pub document: KbDocument,
    pub task_id: String,
    pub duplicate: bool,
}

/// 切片轻量元数据（检索端富化用，不含 `embedding` BLOB，避免把全部向量读进内存）。
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChunkMeta {
    pub id: String,
    pub doc_id: String,
    pub content: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub metadata: String,
}

/// 切片查看项（前端文档查看器用，不含 `embedding` BLOB）。
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChunkView {
    pub chunk_index: i64,
    pub content: String,
    pub token_count: i64,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub metadata: String,
}

/// 知识库统计（文档/切片/token 计数）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KbStats {
    pub doc_count: i64,
    pub chunk_count: i64,
    pub total_tokens: i64,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub doc_id: String,
    pub filename: String,
    pub content: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

/// 多源导入输入（`source_type`：git / url / local_dir）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSourceInput {
    pub source_type: String,
    /// git 仓库地址（source_type=git）
    pub repo_url: Option<String>,
    /// git 分支
    pub branch: Option<String>,
    /// git 访问令牌
    pub token: Option<String>,
    /// 网页地址（source_type=url）
    pub url: Option<String>,
    /// 本地目录路径（source_type=local_dir）
    pub dir_path: Option<String>,
}

/// FTS5 关键词命中（检索端点返回）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FtsHit {
    pub chunk_id: String,
    pub doc_id: String,
    pub content: String,
    /// bm25 排名（越小越相关）
    pub rank: f64,
}

/// 索引构建 / 状态摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub kb_id: String,
    pub status: String,
    pub index_type: String,
    pub chunk_count: i64,
    pub embedding_dim: i64,
    pub index_path: Option<String>,
    /// 因缺少向量而跳过的 chunk 数
    pub skipped: i64,
}

fn default_top_k() -> usize {
    5
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

fn default_max_rounds() -> usize {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_kb_input_roundtrip() {
        let input = CreateKbInput {
            name: "测试".into(),
            description: Some("描述".into()),
            embedding_model: Some("text-embedding-3-small".into()),
            embedding_channel_id: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        let back: CreateKbInput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "测试");
        assert_eq!(back.description.as_deref(), Some("描述"));
    }

    #[test]
    fn test_update_kb_input_defaults_to_none() {
        // 空 JSON 对象也应能反序列化（全字段可选）
        let input: UpdateKbInput = serde_json::from_str("{}").unwrap();
        assert!(input.name.is_none());
        assert!(input.chunk_size.is_none());
        assert!(input.mcp_enabled.is_none());
    }

    #[test]
    fn test_ask_input_serde_defaults() {
        let input: AskInput =
            serde_json::from_str(r#"{"question":"你好"}"#).unwrap();
        assert_eq!(input.top_k, 5);
        assert_eq!(input.model, "gpt-4o");
        assert_eq!(input.max_rounds, 5);
        assert!(!input.deep_research);
        assert!(input.history.is_none());
    }
}
