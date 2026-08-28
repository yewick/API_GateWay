//! MCP JSON-RPC 处理：Streamable HTTP（POST /mcp）+ SSE（GET /mcp/sse）。
//!
//! 通过同一组路由同时支持两种传输：
//! - Streamable HTTP：`POST /mcp` 直接返回 JSON-RPC 响应。
//! - SSE：`GET /mcp/sse` 建立事件流（先返回 `endpoint` 事件告知 POST 地址），之后
//!   `POST /mcp?session_id=...` 的响应经会话通道推送到对应 SSE 流。
//!
//! 13 个知识库工具统一经 [`handle_tool_call`] 分发到知识库领域层（检索 / RAG / CRUD /
//! 文档 / 索引 / 导入），复用 [`crate::services::knowledge`] 已有的实现，不重写逻辑。

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tauri::AppHandle;

use crate::db::repository::Repository;
use crate::server::router::SharedState;
use crate::services::knowledge::models::*;
use crate::services::knowledge::repository::KbRepository;
use crate::services::knowledge::{
    build_index, embed, import_source, process_document, rag, retriever, validate_embedding_config,
    SourceInfo,
};

/// `initialize` 响应中注入的 `instructions`（作为 Agent 首次连接时的系统提示）。
const MCP_INSTRUCTIONS: &str = r#"# YeAPI 知识库 — 本地 RAG + 向量检索

知识库已预建索引：文档已解析、分块、向量化并存入本地 SQLite + HNSW 索引。
所有检索都是本地操作，亚秒级响应。

## 工具使用优先级

1. **ask_knowledge_base** — 首选。直接提问，返回 AI 生成的回答 + 来源引用。
   适合：任何问题、概念理解、代码含义、流程梳理。

2. **search_knowledge_base** — 当需要看原始文本片段，或 ask 回答不够时使用。

3. **list_knowledge_bases** — 首次使用时调用一次，获取可用知识库 ID。

4. **其他工具** — 按需使用（上传文档、管理索引等）。

## 反模式

- ❌ 不要先 search 再自己总结 — 直接用 ask_knowledge_base
- ❌ 不要每次都调 list_knowledge_bases — 缓存第一次的结果
- ❌ 不要对同一问题反复 search 不同关键词

## 代码文件

知识库中的代码文件按符号边界分块（函数/类/方法），每个 chunk 是完整符号。
chunk metadata 包含 symbol_name、symbol_kind、signature，可用于精确过滤。"#;

// ---------------------------------------------------------------------------
// JSON-RPC 数据结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    code: i32,
    message: String,
}

impl McpResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct McpQuery {
    #[serde(default)]
    session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// 13 个工具定义
// ---------------------------------------------------------------------------

pub fn mcp_tools() -> Vec<Value> {
    vec![
        tool(
            "search_knowledge_base",
            "Search across a local knowledge base using hybrid (vector + keyword), vector-only, or keyword-only retrieval. Returns matching text chunks with similarity scores and per-component (vec/kw) score breakdowns. CJK bigram tokenization is used for Chinese queries.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "kb_id": { "type": "string", "description": "Specific KB ID. If omitted, searches all MCP-enabled KBs." },
                    "top_k": { "type": "integer", "description": "Max results (default: 5)", "default": 5 },
                    "search_mode": {
                        "type": "string",
                        "enum": ["hybrid", "vector", "keyword"],
                        "description": "Retrieval mode: hybrid (default), vector (semantic only), keyword (FTS5 only).",
                        "default": "hybrid"
                    },
                    "vector_weight": {
                        "type": "number",
                        "description": "Weight for vector similarity in hybrid mode (0.0-1.0, default: 0.7). Only effective when search_mode=hybrid.",
                        "default": 0.7
                    },
                    "keyword_weight": {
                        "type": "number",
                        "description": "Weight for keyword (FTS5) score in hybrid mode (0.0-1.0, default: 0.3). Only effective when search_mode=hybrid.",
                        "default": 0.3
                    }
                },
                "required": ["query"]
            }),
        ),
        tool(
            "ask_knowledge_base",
            "Ask a question and get an AI-generated answer based on retrieved context (RAG). Returns the answer, source citations, and per-chunk retrieval details (vec/kw scores + code symbols).",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask" },
                    "kb_id": { "type": "string", "description": "KB ID. If omitted, uses all MCP-enabled KBs." },
                    "top_k": { "type": "integer", "description": "Number of chunks to retrieve (default: 5)", "default": 5 },
                    "model": { "type": "string", "description": "LLM model for answer generation" },
                    "search_mode": {
                        "type": "string",
                        "enum": ["hybrid", "vector", "keyword"],
                        "description": "Retrieval mode: hybrid (default), vector (semantic only), keyword (FTS5 only).",
                        "default": "hybrid"
                    },
                    "vector_weight": {
                        "type": "number",
                        "description": "Weight for vector similarity in hybrid mode (0.0-1.0, default: 0.7). Only effective when search_mode=hybrid.",
                        "default": 0.7
                    },
                    "keyword_weight": {
                        "type": "number",
                        "description": "Weight for keyword (FTS5) score in hybrid mode (0.0-1.0, default: 0.3). Only effective when search_mode=hybrid.",
                        "default": 0.3
                    }
                },
                "required": ["question"]
            }),
        ),
        tool(
            "list_knowledge_bases",
            "List all MCP-enabled knowledge bases (id, name, description, counts).",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "read_document",
            "Read the full parsed text of a document by its ID.",
            json!({
                "type": "object",
                "properties": {
                    "doc_id": { "type": "string", "description": "Document ID" }
                },
                "required": ["doc_id"]
            }),
        ),
        tool(
            "get_knowledge_base_stats",
            "Get statistics (document / chunk / token counts) for a knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" }
                },
                "required": ["kb_id"]
            }),
        ),
        tool(
            "create_knowledge_base",
            "Create a new knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Knowledge base name" },
                    "description": { "type": "string", "description": "Knowledge base description" },
                    "embedding_model": { "type": "string", "description": "Embedding model name" },
                    "embedding_channel_id": { "type": "string", "description": "Embedding channel ID" }
                },
                "required": ["name"]
            }),
        ),
        tool(
            "update_knowledge_base",
            "Update a knowledge base (name, description, mcp_enabled, status).",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" },
                    "name": { "type": "string", "description": "New name" },
                    "description": { "type": "string", "description": "New description" },
                    "mcp_enabled": { "type": "integer", "description": "1 = expose via MCP, 0 = hide" },
                    "status": { "type": "integer", "description": "1 = active, 0 = disabled" }
                },
                "required": ["kb_id"]
            }),
        ),
        tool(
            "delete_knowledge_base",
            "Delete a knowledge base and all its documents / chunks.",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" }
                },
                "required": ["kb_id"]
            }),
        ),
        tool(
            "upload_document",
            "上传文档到知识库。文档上传后会自动解析、分块、向量化并建立索引。支持格式: .txt .md .pdf .docx .rs .py .js .ts .go .java 等",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "目标知识库 ID" },
                    "filename": { "type": "string", "description": "文档文件名（含扩展名）" },
                    "content": { "type": "string", "description": "Base64 编码的文件内容" }
                },
                "required": ["kb_id", "filename", "content"]
            }),
        ),
        tool(
            "delete_document",
            "Delete a document and its chunks from a knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "doc_id": { "type": "string", "description": "Document ID" }
                },
                "required": ["doc_id"]
            }),
        ),
        tool(
            "list_documents",
            "List documents in a knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" }
                },
                "required": ["kb_id"]
            }),
        ),
        tool(
            "build_index",
            "Build (or rebuild) the HNSW vector index for a knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" }
                },
                "required": ["kb_id"]
            }),
        ),
        tool(
            "import_source",
            "Import a source (git repo / URL / local directory) into a knowledge base.",
            json!({
                "type": "object",
                "properties": {
                    "kb_id": { "type": "string", "description": "Knowledge base ID" },
                    "source_type": { "type": "string", "description": "git | url | local_dir" },
                    "repo_url": { "type": "string", "description": "git 仓库地址（source_type=git）" },
                    "branch": { "type": "string", "description": "git 分支" },
                    "token": { "type": "string", "description": "git 访问令牌" },
                    "url": { "type": "string", "description": "网页地址（source_type=url）" },
                    "dir_path": { "type": "string", "description": "本地目录路径（source_type=local_dir）" }
                },
                "required": ["kb_id", "source_type"]
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

// ---------------------------------------------------------------------------
// JSON-RPC 分发
// ---------------------------------------------------------------------------

async fn dispatch_jsonrpc_async(shared: &SharedState, req: &McpRequest) -> McpResponse {
    match req.method.as_str() {
        "initialize" => McpResponse::success(req.id.clone(), json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "yeapi-mcp", "version": "0.1.5" },
            "instructions": MCP_INSTRUCTIONS
        })),
        "notifications/initialized" => McpResponse::success(req.id.clone(), json!({})),
        "tools/list" => McpResponse::success(req.id.clone(), json!({ "tools": mcp_tools() })),
        "tools/call" => {
            let tool_name = req
                .params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or(Value::Null);
            match handle_tool_call(shared, tool_name, &args).await {
                Ok(result) => McpResponse::success(req.id.clone(), result),
                Err(e) => McpResponse::error(req.id.clone(), -32603, e),
            }
        }
        "ping" => McpResponse::success(req.id.clone(), json!({})),
        _ => McpResponse::error(req.id.clone(), -32601, format!("Unknown method: {}", req.method)),
    }
}

// ---------------------------------------------------------------------------
// 工具分发 → 知识库领域层
// ---------------------------------------------------------------------------

async fn handle_tool_call(
    shared: &SharedState,
    tool_name: &str,
    args: &Value,
) -> Result<Value, String> {
    let pool = shared.state.db.pool.clone();
    let app = shared.app.clone();
    let repo = KbRepository::new(pool.clone());

    match tool_name {
        "search_knowledge_base" => {
            let query = req_str(args, "query")?.to_string();
            if query.trim().is_empty() {
                return Err("query 不能为空".to_string());
            }
            let kb_id = opt_str(args, "kb_id").unwrap_or("").to_string();
            let top_k = usize_arg(args, "top_k").unwrap_or(5).clamp(1, 100);
            let search_mode = opt_str(args, "search_mode").unwrap_or("hybrid").to_lowercase();
            let vector_weight = f32_arg(args, "vector_weight").unwrap_or(retriever::VECTOR_WEIGHT);
            let keyword_weight = f32_arg(args, "keyword_weight").unwrap_or(retriever::KEYWORD_WEIGHT);
            let db = Repository::new(pool.clone());

            let scored: Vec<retriever::ScoredSearchResult> = if kb_id.is_empty() {
                // 跨库检索：search_all_with_details 已按 search_mode 分派，keyword 模式无需向量化
                let emb = if search_mode == "keyword" {
                    None
                } else {
                    Some(embed_query(&app, &repo, &db, &query, "").await?)
                };
                retriever::search_all_with_details(
                    &repo, &query, emb.as_deref(), top_k, true,
                    vector_weight, keyword_weight, &search_mode,
                )
                .await?
            } else {
                match search_mode.as_str() {
                    "keyword" => retriever::keyword_only_search(&repo, &kb_id, &query, top_k)
                        .await?
                        .into_iter()
                        .map(|r| retriever::ScoredSearchResult {
                            keyword_score: Some(r.score),
                            vector_score: None,
                            result: r,
                        })
                        .collect(),
                    "vector" => {
                        let emb = embed_query(&app, &repo, &db, &query, &kb_id).await?;
                        retriever::vector_search(&repo, &kb_id, &emb, top_k)
                            .await?
                            .into_iter()
                            .map(|r| retriever::ScoredSearchResult {
                                vector_score: Some(r.score),
                                keyword_score: None,
                                result: r,
                            })
                            .collect()
                    }
                    _ => {
                        let emb = embed_query(&app, &repo, &db, &query, &kb_id).await?;
                        retriever::hybrid_search_with_details(
                            &repo, &kb_id, &query, &emb, top_k,
                            vector_weight, keyword_weight,
                        )
                        .await?
                    }
                }
            };

            Ok(tool_text(format_scored_results(&scored)))
        }
        "ask_knowledge_base" => {
            let question = req_str(args, "question")?.to_string();
            if question.trim().is_empty() {
                return Err("question 不能为空".to_string());
            }
            let kb_id = opt_str(args, "kb_id").unwrap_or("").to_string();
            let top_k = usize_arg(args, "top_k").unwrap_or(5).clamp(1, 50);
            let model = opt_str(args, "model").unwrap_or("gpt-4o").to_string();
            let search_mode = opt_str(args, "search_mode").unwrap_or("hybrid").to_lowercase();
            let vector_weight = f32_arg(args, "vector_weight").unwrap_or(retriever::VECTOR_WEIGHT);
            let keyword_weight = f32_arg(args, "keyword_weight").unwrap_or(retriever::KEYWORD_WEIGHT);

            let answer = rag::ask_with_config(
                &pool, &app, &kb_id, &question, &model, top_k, true,
                None, None, None,
                vector_weight, keyword_weight, &search_mode,
            )
            .await?;
            Ok(tool_text(format_rag_answer_with_details(&answer)))
        }
        "list_knowledge_bases" => {
            let kbs = repo.get_all_kbs().await.map_err(|e| e.to_string())?;
            let mcp_kbs: Vec<_> = kbs.into_iter().filter(|k| k.mcp_enabled == 1).collect();
            let text = if mcp_kbs.is_empty() {
                "（没有启用 MCP 的知识库）".to_string()
            } else {
                let mut s = String::from("可用知识库：\n");
                for kb in &mcp_kbs {
                    s.push_str(&format!(
                        "- {} ({}) 文档数:{} 切片数:{}\n",
                        kb.name, kb.id, kb.doc_count, kb.chunk_count
                    ));
                }
                s
            };
            Ok(tool_text(text))
        }
        "read_document" => {
            let doc_id = req_str(args, "doc_id")?.to_string();
            let doc = repo.get_document(&doc_id).await.map_err(|e| e.to_string())?;
            let text = format!("文档：{}\n\n{}", doc.filename, doc.content);
            Ok(tool_text(text))
        }
        "get_knowledge_base_stats" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let stats = repo.get_kb_stats(&kb_id).await.map_err(|e| e.to_string())?;
            let text = format!(
                "知识库统计（{}）：文档 {}，切片 {}，tokens {}",
                kb_id, stats.doc_count, stats.chunk_count, stats.total_tokens
            );
            Ok(tool_text(text))
        }
        "create_knowledge_base" => {
            let name = req_str(args, "name")?.to_string();
            if name.trim().is_empty() {
                return Err("name 不能为空".to_string());
            }
            let description = opt_str(args, "description").map(|s| s.to_string());
            let embedding_model = opt_str(args, "embedding_model").map(|s| s.to_string());
            let embedding_channel_id = opt_str(args, "embedding_channel_id").map(|s| s.to_string());

            let db = Repository::new(pool.clone());
            validate_embedding_config(
                &db,
                embedding_model.as_deref(),
                embedding_channel_id.as_deref(),
            )
            .await?;

            let input = CreateKbInput {
                name,
                description,
                embedding_model,
                embedding_channel_id,
            };
            let kb = repo.create_kb(&input).await.map_err(|e| e.to_string())?;
            Ok(tool_text(
                serde_json::to_string_pretty(&kb).unwrap_or_else(|_| kb.id.clone()),
            ))
        }
        "update_knowledge_base" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let input = UpdateKbInput {
                name: opt_str(args, "name").map(|s| s.to_string()),
                description: opt_str(args, "description").map(|s| s.to_string()),
                embedding_model: None,
                embedding_channel_id: None,
                status: i64_arg(args, "status"),
                mcp_enabled: i64_arg(args, "mcp_enabled"),
                chunk_size: None,
                chunk_overlap: None,
                excluded_dirs: None,
                excluded_files: None,
                included_files: None,
            };
            let kb = repo.update_kb(&kb_id, &input).await.map_err(|e| e.to_string())?;
            Ok(tool_text(
                serde_json::to_string_pretty(&kb).unwrap_or_else(|_| kb.id.clone()),
            ))
        }
        "delete_knowledge_base" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            repo.delete_kb(&kb_id).await.map_err(|e| e.to_string())?;
            Ok(tool_text(format!("已删除知识库 {}", kb_id)))
        }
        "upload_document" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let filename = req_str(args, "filename")?.to_string();
            let content_b64 = req_str(args, "content")?.to_string();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&content_b64)
                .map_err(|e| format!("base64 解码失败: {e}"))?;

            let source = SourceInfo {
                source_type: "upload".to_string(),
                source_url: None,
                source_path: None,
            };
            let outcome = process_document(&pool, &kb_id, &filename, &bytes, &source, &app).await?;
            Ok(tool_text(format!(
                "已上传并入库：{}（切片 {}，tokens {}）",
                outcome.doc_id, outcome.chunk_count, outcome.token_count
            )))
        }
        "delete_document" => {
            let doc_id = req_str(args, "doc_id")?.to_string();
            let doc = repo.get_document(&doc_id).await.map_err(|e| e.to_string())?;
            let kb_id = doc.kb_id.clone();
            repo.delete_chunks_by_doc(&doc_id).await.map_err(|e| e.to_string())?;
            repo.delete_document(&doc_id).await.map_err(|e| e.to_string())?;
            if doc.chunk_count > 0 {
                repo.increment_kb_counts(&kb_id, -1, -doc.chunk_count, -doc.token_count)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(tool_text(format!("已删除文档 {}", doc_id)))
        }
        "list_documents" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let docs = repo.get_documents(&kb_id).await.map_err(|e| e.to_string())?;
            let text = if docs.is_empty() {
                "（无文档）".to_string()
            } else {
                let mut s = format!("知识库 {} 的文档：\n", kb_id);
                for d in &docs {
                    s.push_str(&format!(
                        "- {} ({}) 状态:{} 切片:{} 大小:{}B\n",
                        d.filename, d.id, d.status, d.chunk_count, d.file_size
                    ));
                }
                s
            };
            Ok(tool_text(text))
        }
        "build_index" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let summary = build_index(&app, &pool, &kb_id).await?;
            Ok(tool_text(format!(
                "索引构建完成：{}（状态 {}，切片 {}，维度 {}）",
                summary.kb_id, summary.status, summary.chunk_count, summary.embedding_dim
            )))
        }
        "import_source" => {
            let kb_id = req_str(args, "kb_id")?.to_string();
            let input = ImportSourceInput {
                source_type: req_str(args, "source_type")?.to_string(),
                repo_url: opt_str(args, "repo_url").map(|s| s.to_string()),
                branch: opt_str(args, "branch").map(|s| s.to_string()),
                token: opt_str(args, "token").map(|s| s.to_string()),
                url: opt_str(args, "url").map(|s| s.to_string()),
                dir_path: opt_str(args, "dir_path").map(|s| s.to_string()),
            };
            let summary = import_source(&pool, &kb_id, input, &app).await?;
            Ok(tool_text(format!(
                "导入完成：{}（文件数 {}，状态 {}）",
                summary.source_id, summary.file_count, summary.status
            )))
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

// ---------------------------------------------------------------------------
// 参数解析 + 结果格式化辅助
// ---------------------------------------------------------------------------

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    opt_str(args, key).ok_or_else(|| format!("缺少参数 {}", key))
}

fn usize_arg(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|v| v as usize)
}

fn i64_arg(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

fn f32_arg(args: &Value, key: &str) -> Option<f32> {
    args.get(key).and_then(|v| v.as_f64()).map(|v| v as f32)
}

/// 解析 embedding 模型（单库读 KB 配置，跨库用全局默认）并对 query 向量化。
async fn embed_query(
    app: &AppHandle,
    repo: &KbRepository,
    db: &Repository,
    query: &str,
    kb_id: &str,
) -> Result<Vec<f32>, String> {
    let (model, channel) = if kb_id.is_empty() {
        (rag::default_embedding_model(app), None)
    } else {
        let kb = repo
            .get_kb(kb_id)
            .await
            .map_err(|e| format!("读取知识库失败: {e}"))?;
        (
            kb.embedding_model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| rag::default_embedding_model(app)),
            kb.embedding_channel_id.clone(),
        )
    };
    let vecs = embed(&[query.to_string()], &model, channel.as_deref(), db, None).await?;
    vecs.into_iter()
        .next()
        .ok_or_else(|| "向量化返回空结果".to_string())
}

/// 把纯文本包装成 MCP 工具结果：`{ content: [{ type: "text", text }] }`。
fn tool_text(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn format_scored_results(results: &[retriever::ScoredSearchResult]) -> String {
    if results.is_empty() {
        return "没有找到相关内容。".to_string();
    }
    let mut s = String::from("搜索结果：\n");
    for (i, scored) in results.iter().enumerate() {
        let r = &scored.result;
        let mut head = format!("{}. [{}] (score: {:.2}", i + 1, r.filename, r.score);
        if let Some(vs) = scored.vector_score {
            head.push_str(&format!(", vec: {:.2}", vs));
        }
        if let Some(ks) = scored.keyword_score {
            head.push_str(&format!(", kw: {:.2}", ks));
        }
        head.push(')');
        if scored.vector_score.is_none() && scored.keyword_score.is_some() {
            head.push_str(" [keyword]");
        } else if scored.vector_score.is_some() && scored.keyword_score.is_none() {
            head.push_str(" [vector]");
        }
        s.push_str(&format!("{}\n{}\n", head, r.content));
    }
    s
}

fn format_rag_answer_with_details(answer: &RagAnswer) -> String {
    let mut s = answer.answer.clone();
    if !answer.sources.is_empty() {
        s.push_str("\n\n来源引用：\n");
        for (i, src) in answer.sources.iter().enumerate() {
            s.push_str(&format!(
                "{}. [{}] 相似度 {:.2}\n",
                i + 1,
                src.filename,
                src.score
            ));
        }
    }
    if let Some(details) = &answer.retrieval_details {
        s.push_str("\n--- Retrieval Details ---\n");
        for d in details {
            let mut line = format!("• {} (score: {:.2}", d.filename, d.score);
            if let Some(vs) = d.vector_score {
                line.push_str(&format!(", vec: {:.2}", vs));
            }
            if let Some(ks) = d.keyword_score {
                line.push_str(&format!(", kw: {:.2}", ks));
            }
            if let Some(sym) = &d.symbol_name {
                line.push_str(&format!(", symbol: {}", sym));
                if let Some(kind) = &d.symbol_kind {
                    line.push_str(&format!(" ({})", kind));
                }
            }
            line.push(')');
            s.push_str(&line);
            s.push('\n');
        }
    }
    s
}

// ---------------------------------------------------------------------------
// SSE 会话管理 + 端点
// ---------------------------------------------------------------------------

type SessionSender = mpsc::UnboundedSender<String>;

fn sse_sessions() -> &'static RwLock<HashMap<String, SessionSender>> {
    static SESSIONS: OnceLock<RwLock<HashMap<String, SessionSender>>> = OnceLock::new();
    SESSIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// GET /mcp/sse（及 GET /mcp）—— 建立 SSE 事件流。
/// 先发送 `event: endpoint`（告知客户端后续 POST 地址），之后转发 JSON-RPC 响应并周期性保活。
pub async fn handle_mcp_sse(State(_shared): State<SharedState>) -> Response {
    let session_id = format!("sess_{}", uuid::Uuid::new_v4().simple());
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    sse_sessions().write().await.insert(session_id.clone(), tx);

    let stream = async_stream::stream! {
        let endpoint_url = format!("/mcp?session_id={}", session_id);
        yield Ok::<_, std::io::Error>(
            format!("event: endpoint\ndata: {}\n\n", endpoint_url).into_bytes(),
        );

        let mut keepalive = tokio::time::interval(Duration::from_secs(15));
        keepalive.tick().await;

        loop {
            tokio::select! {
                Some(msg) = rx.recv() => {
                    yield Ok::<_, std::io::Error>(format!("data: {}\n\n", msg).into_bytes());
                }
                _ = keepalive.tick() => {
                    yield Ok::<_, std::io::Error>(b": keepalive\n\n".to_vec());
                }
            }
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream")
        .header("Cache-Control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// POST /mcp —— JSON-RPC 分发。
/// - 无 `session_id`：Streamable HTTP，直接返回 JSON-RPC 响应。
/// - 有 `session_id`：SSE 模式，把响应推送到对应 SSE 流。
pub async fn handle_mcp(
    State(shared): State<SharedState>,
    Query(params): Query<McpQuery>,
    body: Bytes,
) -> Response {
    let text = String::from_utf8_lossy(&body);
    let req: McpRequest = match serde_json::from_str(text.trim()) {
        Ok(r) => r,
        Err(_) => {
            return Json(McpResponse::error(None, -32700, "Parse error")).into_response();
        }
    };

    // 通知（无 id）：JSON-RPC 规范不期待响应。
    if req.id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    // Streamable HTTP：无 session_id → 直接返回响应。
    if params.session_id.is_none() {
        let response = dispatch_jsonrpc_async(&shared, &req).await;
        return Json(response).into_response();
    }

    // SSE 模式：先取发送端（释放读锁），再分发并推送响应。
    let session_id = params.session_id.as_deref().unwrap_or("");
    let sender = sse_sessions().read().await.get(session_id).cloned();
    if let Some(tx) = sender {
        let response = dispatch_jsonrpc_async(&shared, &req).await;
        let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
        return StatusCode::ACCEPTED.into_response();
    }

    // 会话不存在：回退为直接响应。
    let response = dispatch_jsonrpc_async(&shared, &req).await;
    Json(response).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tools_has_13_tools() {
        let tools = mcp_tools();
        assert_eq!(tools.len(), 13);
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for expected in [
            "search_knowledge_base",
            "ask_knowledge_base",
            "list_knowledge_bases",
            "read_document",
            "get_knowledge_base_stats",
            "create_knowledge_base",
            "update_knowledge_base",
            "delete_knowledge_base",
            "upload_document",
            "delete_document",
            "list_documents",
            "build_index",
            "import_source",
        ] {
            assert!(names.contains(&expected), "缺少工具 {}", expected);
        }
    }

    #[test]
    fn test_mcp_request_parses_with_default_params() {
        let req: McpRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.params, Value::Null);
        assert_eq!(req.id, Some(json!(1)));
    }

    #[test]
    fn test_mcp_response_success_and_error_shape() {
        let ok = McpResponse::success(Some(json!(7)), json!({ "tools": [] }));
        let v = serde_json::to_value(&ok).unwrap();
        assert_eq!(v["jsonrpc"], json!("2.0"));
        assert_eq!(v["id"], json!(7));
        assert!(v.get("result").is_some());
        assert!(v.get("error").is_none());

        let err = McpResponse::error(Some(json!(7)), -32601, "Method not found");
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["error"]["code"], json!(-32601));
        assert!(v.get("result").is_none());
    }
}
