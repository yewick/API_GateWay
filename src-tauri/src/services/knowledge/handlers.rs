//! 知识库 HTTP 端点（`/api/kb*`）。
//!
//! 统一从 `State<SharedState>` 取数据库连接池，错误映射为 `(StatusCode, Json)`。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::server::router::SharedState;

use super::models::*;
use super::{parser, splitter};
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

    // 4. 解析文档
    let parsed = parser::parse_document(&input.filename, &bytes)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;

    // 5. 分块
    let chunks = splitter::split_document(&parsed, &splitter::SplitConfig::default());

    // 6. 组装文档 + 切片
    let now = crate::utils::time::now_iso();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let total_tokens: i64 = chunks.iter().map(|c| c.token_count as i64).sum();

    let doc = KbDocument {
        id: doc_id.clone(),
        kb_id: kb_id.clone(),
        filename: input.filename.clone(),
        file_path: None,
        file_type: parsed.file_type.clone(),
        file_size: bytes.len() as i64,
        content_hash: hash,
        chunk_count: chunks.len() as i64,
        token_count: total_tokens,
        status: "ready".to_string(),
        error_message: None,
        source_type: "upload".to_string(),
        source_url: None,
        source_path: None,
        doc_meta: "{}".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let kb_chunks: Vec<KbChunk> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let meta_json = serde_json::to_string(&c.metadata).unwrap_or_else(|_| "{}".to_string());
            KbChunk {
                id: uuid::Uuid::new_v4().to_string(),
                doc_id: doc_id.clone(),
                kb_id: kb_id.clone(),
                chunk_index: i as i64,
                content: c.content.clone(),
                token_count: c.token_count as i64,
                embedding: None,
                embedding_dim: 0,
                metadata: meta_json,
                symbol_name: c.metadata.symbol_name.clone(),
                symbol_kind: c.metadata.symbol_kind.clone(),
                created_at: now.clone(),
            }
        })
        .collect();

    // 7. 落库并更新计数
    let saved_doc = repo.create_document(&doc).await.map_err(db_err)?;
    repo.insert_chunks_bulk(&kb_chunks).await.map_err(db_err)?;
    repo.increment_kb_counts(&kb_id, 1, chunks.len() as i64, total_tokens)
        .await
        .map_err(db_err)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "document": saved_doc,
            "chunk_count": chunks.len(),
        })),
    ))
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
