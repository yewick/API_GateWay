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

use crate::db::repository::Repository;
use crate::server::router::SharedState;

use super::embedder;
use super::importer;
use super::models::*;
use super::processor;
use super::rag;
use super::repository::KbRepository;
use super::retriever;

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
    if doc.kb_id != kb_id {
        return Err(err(StatusCode::NOT_FOUND, "文档不属于该知识库"));
    }
    repo.delete_chunks_by_doc(&doc_id).await.map_err(db_err)?;
    repo.delete_document(&doc_id).await.map_err(db_err)?;
    // 只有「已入库（有切片）」的文档才计入过 doc_count，才需要回减；
    // parsing / awaiting_review / 解析失败 的文档从未计数（chunk_count=0），不能回减。
    if doc.chunk_count > 0 {
        repo.increment_kb_counts(&kb_id, -1, -doc.chunk_count, -doc.token_count)
            .await
            .map_err(db_err)?;
    }
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

/// POST /api/kb/{id}/documents —— 上传文档（base64 内容）。
/// 立即创建文档记录（status=parsing）+ 解析任务，后台解析；返回 `{ document, task_id }`。
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

    // 4. 先落一条「解析中」文档记录（content 空，后台解析后回填）
    let now = crate::utils::time::now_iso();
    let doc_id = crate::utils::id::new_id();
    let doc = KbDocument {
        id: doc_id.clone(),
        kb_id: kb_id.clone(),
        filename: input.filename.clone(),
        file_path: None,
        file_type: String::new(),
        file_size: bytes.len() as i64,
        content_hash: hash,
        content: String::new(),
        chunk_count: 0,
        token_count: 0,
        status: "parsing".to_string(),
        error_message: None,
        source_type: "upload".to_string(),
        source_url: None,
        source_path: None,
        doc_meta: "{}".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let doc = repo.create_document(&doc).await.map_err(db_err)?;

    // 5. 建解析任务并后台执行
    let task = KbTask {
        id: crate::utils::id::new_id(),
        kb_id: kb_id.clone(),
        doc_id: Some(doc_id.clone()),
        task_type: "parse".to_string(),
        status: "running".to_string(),
        progress: 0,
        total_items: 0,
        done_items: 0,
        error_message: None,
        created_at: crate::utils::time::now_iso(),
        completed_at: None,
    };
    let task = repo.create_task(&task).await.map_err(db_err)?;

    let pool = shared.state.db.pool.clone();
    let app = shared.app.clone();
    let kb = kb_id.clone();
    let filename = input.filename.clone();
    let doc_for_spawn = doc_id.clone();
    let task_for_spawn = task.id.clone();
    tauri::async_runtime::spawn(async move {
        processor::parse_document_background(
            &pool,
            &kb,
            &filename,
            &bytes,
            &doc_for_spawn,
            &task_for_spawn,
            &app,
        )
        .await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "document": doc,
            "task_id": task.id,
        })),
    ))
}

/// POST /api/kb/{id}/documents/{doc_id}/ingest —— 确认入库（分块→向量化→索引→ready）
pub async fn ingest_document(
    State(shared): State<SharedState>,
    Path((kb_id, doc_id)): Path<(String, String)>,
) -> ApiResult {
    let outcome =
        processor::ingest_document(&shared.state.db.pool, &kb_id, &doc_id, &shared.app)
            .await
            .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok((StatusCode::OK, Json(json!(outcome))))
}

/// GET /api/kb/{id}/documents/{doc_id} —— 单文档 + 其最新解析任务进度
pub async fn get_document(
    State(shared): State<SharedState>,
    Path((kb_id, doc_id)): Path<(String, String)>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let doc = repo.get_document(&doc_id).await.map_err(db_err)?;
    if doc.kb_id != kb_id {
        return Err(err(StatusCode::NOT_FOUND, "文档不属于该知识库"));
    }
    let task = repo.get_latest_task_by_doc(&doc_id).await.map_err(db_err)?;
    Ok((
        StatusCode::OK,
        Json(json!({ "document": doc, "task": task })),
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

/// GET /api/kb/search?kb_id=&query=&top_k=&symbol_kind= —— 混合检索（kb_id 可空=全局）
pub async fn search(
    State(shared): State<SharedState>,
    Query(params): Query<SearchQueryParams>,
) -> ApiResult {
    let query = params.query.trim().to_string();
    if query.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "缺少 query 参数"));
    }
    let top_k = params.top_k.unwrap_or(5).clamp(1, 100);
    let kb_id = params.kb_id.clone().unwrap_or_default();
    let pool = shared.state.db.pool.clone();
    let repo = KbRepository::new(pool.clone());
    let db = Repository::new(pool.clone());

    // 确定 embedding 模型与指定渠道（kb_id 为空 → 全局默认）
    let (embedding_model, embedding_channel_id) = if kb_id.is_empty() {
        (rag::DEFAULT_EMBEDDING_MODEL.to_string(), None)
    } else {
        let kb = repo.get_kb(&kb_id).await.map_err(db_err)?;
        (
            kb.embedding_model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| rag::DEFAULT_EMBEDDING_MODEL.to_string()),
            kb.embedding_channel_id.clone(),
        )
    };

    let vecs = embedder::embed(&[query.clone()], &embedding_model, embedding_channel_id.as_deref(), &db)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let query_emb = vecs
        .into_iter()
        .next()
        .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "向量化返回空结果"))?;

    let mut results = if kb_id.is_empty() {
        retriever::search_all(&repo, &query, &query_emb, top_k, true).await
    } else {
        retriever::hybrid_search(
            &repo,
            &kb_id,
            &query,
            &query_emb,
            top_k,
            retriever::VECTOR_WEIGHT,
            retriever::KEYWORD_WEIGHT,
        )
        .await
    }
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some(kind) = params.symbol_kind.as_deref() {
        results = retriever::filter_by_symbol(results, kind);
    }

    Ok((StatusCode::OK, Json(json!({ "data": results }))))
}

/// POST /api/kb/ask —— RAG 问答（非流式）
pub async fn ask(
    State(shared): State<SharedState>,
    Json(input): Json<AskInput>,
) -> ApiResult {
    let question = input.question.trim().to_string();
    if question.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "缺少 question 参数"));
    }
    let kb_id = input.kb_id.clone().unwrap_or_default();
    let answer = rag::ask(
        &shared.state.db.pool,
        &shared.app,
        &kb_id,
        &question,
        &input.model,
        input.top_k,
        false,
        input.history.as_deref(),
        input.context_limit,
    )
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((StatusCode::OK, Json(json!(answer))))
}

/// GET /api/kb/{id}/conversations —— 对话历史
pub async fn list_conversations(
    State(shared): State<SharedState>,
    Path(kb_id): Path<String>,
) -> ApiResult {
    let repo = KbRepository::new(shared.state.db.pool.clone());
    let convs = repo.get_conversations(&kb_id).await.map_err(db_err)?;
    Ok((StatusCode::OK, Json(json!({ "data": convs }))))
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

/// 混合检索查询参数（`kb_id` 可空 = 全局；`symbol_kind` 可选过滤）
#[derive(serde::Deserialize)]
pub struct SearchQueryParams {
    kb_id: Option<String>,
    query: String,
    top_k: Option<usize>,
    symbol_kind: Option<String>,
}
