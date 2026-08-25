//! 知识库 HTTP 端点（`/api/kb*`）。
//!
//! 统一从 `State<SharedState>` 取数据库连接池，错误映射为 `(StatusCode, Json)`。

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::server::router::SharedState;

use super::importer;
use super::models::*;
use super::processor::{self, SourceInfo};
use super::repository::KbRepository;

/// 统一返回类型：成功 = (状态码, JSON)，失败 = (状态码, JSON 错误体)
type ApiResult = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (code, Json(json!({ "error": msg.into() })))
}

fn db_err(e: sqlx::Error) -> (StatusCode, Json<Value>) {
    let code = match e {
        sqlx::Error::RowNotFound => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (code, Json(json!({ "error": e.to_string() })))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// GET /api/kb —— 列出所有知识库
pub async fn list_knowledge_bases(State(shared): State<SharedState>) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let kbs = repo.get_all_kbs().await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "data": kbs }))))
}

/// POST /api/kb —— 创建知识库
pub async fn create_knowledge_base(
    State(shared): State<SharedState>,
    Json(input): Json<CreateKbInput>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let kb = repo.create_kb(&input).await.map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(json!(kb))))
}

/// GET /api/kb/{id} —— 获取单个知识库
pub async fn get_knowledge_base(
    State(shared): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let kb = repo.get_kb(&id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!(kb))))
}

/// PUT /api/kb/{id} —— 更新知识库
pub async fn update_knowledge_base(
    State(shared): State<SharedState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateKbInput>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let kb = repo.update_kb(&id, &input).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!(kb))))
}

/// DELETE /api/kb/{id} —— 删除知识库（级联删除文档/切片）
pub async fn delete_knowledge_base(
    State(shared): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    repo.delete_kb(&id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "deleted": id }))))
}

/// GET /api/kb/{id}/documents —— 列出知识库文档
pub async fn list_documents(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let docs = repo.get_documents(&kb_id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "data": docs }))))
}

/// DELETE /api/kb/{id}/documents/{doc_id} —— 删除文档及其切片，并回减计数
pub async fn delete_document(
    State(shared): State<SharedState>,
    Path((kb_id, doc_id)): Path<(String, String)>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let doc = repo.get_document(&doc_id).await.map_err(db_err)?;
    repo.delete_chunks_by_doc(&doc_id).await.map_err(db_err)?;
    repo.delete_document(&doc_id).await.map_err(db_err)?;
    repo.increment_kb_counts(&kb_id, -1, -doc.chunk_count, -doc.token_count)
        .await
        .map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "deleted": doc_id }))))
}

/// GET /api/kb/{id}/stats —— 知识库统计
pub async fn kb_stats(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let stats = repo.get_kb_stats(&kb_id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!(stats))))
}

/// POST /api/kb/{id}/documents —— 上传文档（base64 内容）→ 解析 → 分块 → 落库
pub async fn upload_document(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
    Json(input): Json<UploadDocumentInput>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());

    // 1. base64 解码
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&input.content)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("base64 解码失败: {}", e)))?;

    // 2. SHA256 内容哈希（去重）
    let hash = sha256_hex(&bytes);

    // 3. 同一知识库内相同内容跳过
    if let Some(existing) = repo
        .get_document_by_hash(&kb_id, &hash)
        .await
        .map_err(db_err)?
    {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "document": existing,
                "duplicate": true,
            })),
        ));
    }

    // 4. 走 processor 完整流水线（解析→分块→落库→向量化→状态/事件→增量索引）
    let source = SourceInfo {
        source_type: "upload".to_string(),
        source_url: None,
        source_path: None,
    };
    let outcome = processor::process_document(
        &shared.state.db.pool,
        &kb_id,
        &input.filename,
        &bytes,
        &source,
        &shared.app,
    )
    .await
    .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;

    let doc = repo.get_document(&outcome.doc_id).await.map_err(db_err)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "document": doc,
            "chunk_count": outcome.chunk_count,
            "token_count": outcome.token_count,
            "embedding_dim": outcome.embedding_dim,
        })),
    ))
}

/// GET /api/kb/{id}/documents/{doc_id}/content —— 导出文档完整解析文本（无损 Markdown/原文）
pub async fn get_document_content(
    State(shared): State<SharedState>,
    Path((kb_id, doc_id)): Path<(String, String)>,
) -> Response {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let doc = match repo.get_document(&doc_id).await {
        Ok(d) => d,
        Err(e) => return db_err(e).into_response(),
    };
    if doc.kb_id != kb_id {
        return err(StatusCode::NOT_FOUND, "文档不属于该知识库").into_response();
    }
    let ct = if doc.file_type == "markdown" {
        "text/markdown; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    };
    ([(header::CONTENT_TYPE, ct)], doc.content).into_response()
}

/// GET /api/kb/status —— 知识库聚合状态
pub async fn kb_status(State(shared): State<SharedState>) -> ApiResult {
    let pool = shared.state.db.pool.clone();
    let kb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_knowledge_bases")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_documents")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_chunks")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    Ok((
        StatusCode::OK,
        Json(json!({
            "service": "knowledge",
            "name": "知识库",
            "registered": true,
            "knowledge_bases": kb_count,
            "documents": doc_count,
            "chunks": chunk_count,
        })),
    ))
}

/// POST /api/kb/{id}/index —— 全量（重）构建 HNSW 索引
pub async fn build_index(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let summary = processor::build_index(&shared.app, &shared.state.db.pool, &kb_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((StatusCode::OK, Json(json!(summary))))
}

/// GET /api/kb/{id}/index —— 查询索引状态
pub async fn get_index(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let summary = processor::get_index_status(&shared.state.db.pool, &kb_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((StatusCode::OK, Json(json!(summary))))
}

/// GET /api/kb/{id}/search?query=...&top_k=... —— FTS5 关键词搜索
pub async fn search_fts(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
    Query(params): Query<SearchParams>,
) -> ApiResult {
    let query = params.query.trim().to_string();
    if query.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "缺少 query 参数"));
    }
    let top_k = params.top_k.unwrap_or(10).clamp(1, 100);
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let hits = repo
        .search_fts(&kb_id, &query, top_k)
        .await
        .map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "data": hits }))))
}

/// POST /api/kb/{id}/sources —— 多源导入（git / url / local_dir）
pub async fn import_source(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
    Json(input): Json<ImportSourceInput>,
) -> ApiResult {
    let summary = importer::import_source(&shared.state.db.pool, &kb_id, input, &shared.app)
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok((StatusCode::CREATED, Json(json!(summary))))
}

/// GET /api/kb/{id}/sources —— 列出导入源
pub async fn list_sources(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let sources = repo.list_sources(&kb_id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "data": sources }))))
}

/// DELETE /api/kb/{id}/sources/{source_id} —— 删除导入源记录
pub async fn delete_source(
    State(shared): State<SharedState>,
    Path((_kb_id, source_id)): Path<(String, String)>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    repo.delete_source(&source_id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "deleted": source_id }))))
}

/// 搜索查询参数
#[derive(serde::Deserialize)]
pub struct SearchParams {
    query: String,
    top_k: Option<i64>,
}
