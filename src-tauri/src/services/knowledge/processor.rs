//! 处理流水线：文档的完整生命周期
//! 解析 → 分块 → 落库 → 向量化 → 状态/统计/事件 → 增量索引。

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::repository::Repository;

use super::embedder;
use super::index::{HnswIndex, IndexStore};
use super::models::*;
use super::parser;
use super::pdf;
use super::repository::KbRepository;
use super::splitter::{self, SplitConfig};

/// 文档来源信息（上传 / git / url / local_dir）。
pub struct SourceInfo {
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_path: Option<String>,
}

/// 单文档处理结果。
#[derive(Debug, serde::Serialize)]
pub struct ProcessOutcome {
    pub doc_id: String,
    pub chunk_count: usize,
    pub token_count: i64,
    pub embedding_dim: Option<i64>,
}

/// 处理一个文档（解析+入库一体）：解析、分块、落库、向量化、更新状态与计数、发送事件、增量索引。
/// 供批量导入（git/url/local_dir）使用，自动入库、不做人工预览。
/// 向量化失败时文档落为 `failed`（chunk 已持久化、embedding=None），并返回 Err。
pub async fn process_document(
    pool: &SqlitePool,
    kb_id: &str,
    filename: &str,
    content: &[u8],
    source: &SourceInfo,
    app: &AppHandle,
) -> Result<ProcessOutcome, String> {
    let repo = KbRepository::new(pool.clone());

    // 1. 解析
    let parsed = parser::parse_document(filename, content, None).await?;

    // 2. 先落一条文档记录（status=processing，content 已就绪，chunk 待入库）
    let now = crate::utils::time::now_iso();
    let doc_id = crate::utils::id::new_id();
    let doc = KbDocument {
        id: doc_id.clone(),
        kb_id: kb_id.to_string(),
        filename: filename.to_string(),
        file_path: source.source_path.clone(),
        file_type: parsed.file_type.clone(),
        file_size: content.len() as i64,
        content_hash: sha256_hex(content),
        content: parsed.text.clone(),
        chunk_count: 0,
        token_count: 0,
        status: "processing".to_string(),
        error_message: None,
        source_type: source.source_type.clone(),
        source_url: source.source_url.clone(),
        source_path: source.source_path.clone(),
        doc_meta: "{}".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    repo.create_document(&doc)
        .await
        .map_err(|e| format!("创建文档失败: {e}"))?;

    // 3. 分块 → 落库 → 向量化 → 索引 → ready
    ingest_into_kb(pool, kb_id, &doc_id, &parsed, app).await
}

/// 上传流水线的后台解析段：解析文档 → 回写 content/file_type → `awaiting_review`。
/// 进度经 `kb_tasks` 落库并逐次发 `document-progress` 事件；不写切片、不改计数。
/// `doc_id` 对应的文档已在调用方创建（status=parsing、content 空）。
pub async fn parse_document_background(
    pool: &SqlitePool,
    kb_id: &str,
    filename: &str,
    content: &[u8],
    doc_id: &str,
    task_id: &str,
    app: &AppHandle,
) {
    let repo = KbRepository::new(pool.clone());

    // 进度通道：解析器上报 → 转发（落库 kb_tasks + 发 document-progress 事件）
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<pdf::ParseProgress>();
    {
        let kb = kb_id.to_string();
        let doc = doc_id.to_string();
        let task = task_id.to_string();
        let app = app.clone();
        let pool = pool.clone();
        tauri::async_runtime::spawn(async move {
            let repo = KbRepository::new(pool);
            while let Some(p) = rx.recv().await {
                let progress = percent(p.done, p.total);
                let _ = repo
                    .update_task_progress(&task, progress, p.done as i64, p.total as i64)
                    .await;
                let _ = app.emit(
                    "document-progress",
                    serde_json::json!({
                        "kb_id": kb,
                        "doc_id": doc,
                        "task_id": task,
                        "stage": p.stage,
                        "progress": progress,
                        "done": p.done,
                        "total": p.total,
                    }),
                );
            }
        });
    }

    match parser::parse_document(filename, content, Some(tx)).await {
        Ok(parsed) => {
            let _ = repo
                .update_document_content(doc_id, &parsed.text, &parsed.file_type, "awaiting_review")
                .await;
            let _ = repo.complete_task(task_id).await;
            let _ = app.emit(
                "document-parsed",
                serde_json::json!({
                    "kb_id": kb_id,
                    "doc_id": doc_id,
                    "status": "awaiting_review",
                }),
            );
        }
        Err(e) => {
            let _ = repo.update_document_status(doc_id, "failed", Some(&e)).await;
            let _ = repo.fail_task(task_id, &e).await;
            let _ = app.emit(
                "document-failed",
                serde_json::json!({
                    "kb_id": kb_id,
                    "doc_id": doc_id,
                    "status": "failed",
                    "error": e,
                }),
            );
        }
    }
}

/// 入库段：把已解析（`awaiting_review`）的文档分块、向量化、增量索引，置 `ready`。
pub async fn ingest_document(
    pool: &SqlitePool,
    kb_id: &str,
    doc_id: &str,
    app: &AppHandle,
) -> Result<ProcessOutcome, String> {
    let repo = KbRepository::new(pool.clone());
    let doc = repo
        .get_document(doc_id)
        .await
        .map_err(|e| format!("读取文档失败: {e}"))?;
    if doc.kb_id != kb_id {
        return Err("文档不属于该知识库".to_string());
    }
    if doc.status != "awaiting_review" {
        return Err(format!(
            "文档状态为 {}，不能入库（需为 awaiting_review）",
            doc.status
        ));
    }

    // 用存储的 content/file_type 重构 ParsedDocument（不重复解析）
    let ext = parser::extension(&doc.filename);
    let language = if doc.file_type == "code" {
        parser::determine_language(&ext)
    } else {
        None
    };
    let parsed = parser::ParsedDocument {
        text: doc.content.clone(),
        file_type: doc.file_type.clone(),
        language,
    };

    repo.update_document_status(doc_id, "processing", None)
        .await
        .map_err(|e| e.to_string())?;

    ingest_into_kb(pool, kb_id, doc_id, &parsed, app).await
}

/// 对已存在（status=processing、content 已就绪）的文档执行：分块 → 落库 → 向量化 → 索引 → ready。
/// 向量化失败时文档落为 `failed`（chunk 已持久化、embedding=None），并返回 Err。
async fn ingest_into_kb(
    pool: &SqlitePool,
    kb_id: &str,
    doc_id: &str,
    parsed: &parser::ParsedDocument,
    app: &AppHandle,
) -> Result<ProcessOutcome, String> {
    let repo = KbRepository::new(pool.clone());
    let db = Repository::new(pool.clone());

    // 1. 取知识库配置
    let kb = repo
        .get_kb(kb_id)
        .await
        .map_err(|e| format!("读取知识库失败: {e}"))?;

    // 2. 分块
    let config = SplitConfig {
        chunk_size: kb.chunk_size.max(1) as usize,
        chunk_overlap: kb.chunk_overlap.max(0) as usize,
    };
    let chunks = splitter::split_document(parsed, &config);
    let total_tokens: i64 = chunks.iter().map(|c| c.token_count as i64).sum();

    // 3. 切片落库 + 回写文档 chunk 统计
    let now = crate::utils::time::now_iso();
    let kb_chunks: Vec<KbChunk> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let meta_json =
                serde_json::to_string(&c.metadata).unwrap_or_else(|_| "{}".to_string());
            KbChunk {
                id: crate::utils::id::new_id(),
                doc_id: doc_id.to_string(),
                kb_id: kb_id.to_string(),
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

    repo.insert_chunks_bulk(&kb_chunks)
        .await
        .map_err(|e| format!("写入切片失败: {e}"))?;
    repo.update_document_chunk_stats(doc_id, chunks.len() as i64, total_tokens)
        .await
        .map_err(|e| e.to_string())?;

    // 4. 向量化（未配置模型 → 视为失败，文档不可向量检索）
    let model = match kb.embedding_model.clone() {
        Some(m) if !m.trim().is_empty() => m,
        _ => {
            return finish_failed(
                &repo,
                doc_id,
                kb_id,
                total_tokens,
                chunks.len(),
                "知识库未配置 embedding_model，无法向量化".to_string(),
                app,
            )
            .await;
        }
    };

    let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
    let vecs = match embedder::embed(&texts, &model, kb.embedding_channel_id.as_deref(), &db, None).await {
        Ok(v) => v,
        Err(e) => {
            return finish_failed(&repo, doc_id, kb_id, total_tokens, chunks.len(), e, app).await;
        }
    };

    // 5. 校验并回写向量
    if vecs.len() != kb_chunks.len() {
        return finish_failed(
            &repo,
            doc_id,
            kb_id,
            total_tokens,
            chunks.len(),
            format!("向量化返回 {} 个向量，与切片数 {} 不一致", vecs.len(), kb_chunks.len()),
            app,
        )
        .await;
    }

    let dim = vecs[0].len() as i64;
    if dim == 0 {
        return finish_failed(
            &repo,
            doc_id,
            kb_id,
            total_tokens,
            chunks.len(),
            "向量维度为 0".to_string(),
            app,
        )
        .await;
    }
    if kb.embedding_dim != 0 && kb.embedding_dim != dim {
        return finish_failed(
            &repo,
            doc_id,
            kb_id,
            total_tokens,
            chunks.len(),
            format!("向量维度 {dim} 与知识库已记录的 {} 不一致", kb.embedding_dim),
            app,
        )
        .await;
    }

    for (chunk, vec) in kb_chunks.iter().zip(vecs.iter()) {
        if let Err(e) = repo.update_chunk_embedding(&chunk.id, vec).await {
            return finish_failed(
                &repo,
                doc_id,
                kb_id,
                total_tokens,
                chunks.len(),
                format!("写入向量失败: {e}"),
                app,
            )
            .await;
        }
    }
    // 首次向量化：记录维度
    if kb.embedding_dim == 0 {
        if let Err(e) = sqlx::query(
            "UPDATE kb_knowledge_bases SET embedding_dim = ?, updated_at = ? WHERE id = ?",
        )
        .bind(dim)
        .bind(crate::utils::time::now_iso())
        .bind(kb_id)
        .execute(pool)
        .await
        {
            return finish_failed(
                &repo,
                doc_id,
                kb_id,
                total_tokens,
                chunks.len(),
                format!("记录向量维度失败: {e}"),
                app,
            )
            .await;
        }
    }

    // 6. 增量索引（已构建过索引的知识库才需要；失败不影响文档就绪）
    if kb.index_status != "none" {
        if let Err(e) = append_to_index(app, pool, kb_id, &kb_chunks, &vecs).await {
            tracing::warn!("增量索引失败（不影响文档就绪）: {e}");
        }
    }

    // 7. 成功
    repo.update_document_status(doc_id, "ready", None)
        .await
        .map_err(|e| e.to_string())?;
    repo.increment_kb_counts(kb_id, 1, chunks.len() as i64, total_tokens)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "document-processed",
        serde_json::json!({
            "kb_id": kb_id,
            "doc_id": doc_id,
            "status": "ready",
            "chunk_count": chunks.len(),
        }),
    );

    Ok(ProcessOutcome {
        doc_id: doc_id.to_string(),
        chunk_count: chunks.len(),
        token_count: total_tokens,
        embedding_dim: Some(dim),
    })
}

/// 进度百分比：`total=0`（未知）→ 0（前端按不定进度处理）。
fn percent(done: u64, total: u64) -> i64 {
    if total == 0 {
        return 0;
    }
    ((done as f64 / total as f64) * 100.0).min(100.0) as i64
}

/// 向量化失败时的收尾：文档置 failed、计数正常回写、发事件，并返回 Err。
async fn finish_failed(
    repo: &KbRepository,
    doc_id: &str,
    kb_id: &str,
    token_count: i64,
    chunk_count: usize,
    err: String,
    app: &AppHandle,
) -> Result<ProcessOutcome, String> {
    let _ = repo
        .update_document_status(doc_id, "failed", Some(&err))
        .await;
    let _ = repo
        .increment_kb_counts(kb_id, 1, chunk_count as i64, token_count)
        .await;
    let _ = app.emit(
        "document-failed",
        serde_json::json!({
            "kb_id": kb_id,
            "doc_id": doc_id,
            "status": "failed",
            "error": err,
        }),
    );
    Err(err)
}

/// 把新 chunk 的向量追加到现有索引（位置从现有 `ids.len()` 起），并更新索引元数据。
async fn append_to_index(
    app: &AppHandle,
    pool: &SqlitePool,
    kb_id: &str,
    chunks: &[KbChunk],
    vecs: &[Vec<f32>],
) -> Result<(), String> {
    let repo = KbRepository::new(pool.clone());
    let path = index_dir(app)?.join(format!("kb_{kb_id}.hnsw"));

    let mut store = if path.exists() {
        IndexStore::load(&path).unwrap_or_else(|_| IndexStore {
            ids: Vec::new(),
            index: HnswIndex::default(),
        })
    } else {
        IndexStore {
            ids: Vec::new(),
            index: HnswIndex::default(),
        }
    };

    let start = store.ids.len();
    for (i, (chunk, vec)) in chunks.iter().zip(vecs.iter()).enumerate() {
        store.index.insert(start + i, vec.clone())?;
        store.ids.push(chunk.id.clone());
    }
    store.save(&path)?;

    let meta = KbIndexMeta {
        kb_id: kb_id.to_string(),
        index_type: "hnsw".to_string(),
        embedding_dim: store.index.dim as i64,
        chunk_count: store.index.nodes.len() as i64,
        index_path: Some(path.to_string_lossy().to_string()),
        built_at: Some(crate::utils::time::now_iso()),
        status: "ready".to_string(),
    };
    repo.upsert_index_meta(&meta)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 全量（重）构建 HNSW 索引：取全部已向量化 chunk，按确定性顺序组装并落盘。
pub async fn build_index(
    app: &AppHandle,
    pool: &SqlitePool,
    kb_id: &str,
) -> Result<IndexSummary, String> {
    let repo = KbRepository::new(pool.clone());
    repo.update_index_status(kb_id, "building")
        .await
        .map_err(|e| e.to_string())?;

    let pairs = repo
        .get_chunks_with_embeddings(kb_id)
        .await
        .map_err(|e| format!("读取已向量化切片失败: {e}"))?;

    let mut ids: Vec<String> = Vec::with_capacity(pairs.len());
    let mut items: Vec<(usize, Vec<f32>)> = Vec::with_capacity(pairs.len());
    let mut skipped = 0i64;
    let mut pos = 0usize;
    for (id, vec) in pairs {
        if vec.is_empty() {
            skipped += 1;
            continue;
        }
        ids.push(id);
        items.push((pos, vec));
        pos += 1;
    }

    let mut index = HnswIndex::default();
    index.build_with_progress(&items, |_, _| {})?;

    let path = index_dir(app)?.join(format!("kb_{kb_id}.hnsw"));
    let store = IndexStore { ids, index };
    store.save(&path)?;

    let meta = KbIndexMeta {
        kb_id: kb_id.to_string(),
        index_type: "hnsw".to_string(),
        embedding_dim: store.index.dim as i64,
        chunk_count: store.index.nodes.len() as i64,
        index_path: Some(path.to_string_lossy().to_string()),
        built_at: Some(crate::utils::time::now_iso()),
        status: "ready".to_string(),
    };
    repo.upsert_index_meta(&meta)
        .await
        .map_err(|e| e.to_string())?;
    repo.update_index_status(kb_id, "ready")
        .await
        .map_err(|e| e.to_string())?;

    let _ = app.emit(
        "index-built",
        serde_json::json!({
            "kb_id": kb_id,
            "status": "ready",
            "chunk_count": meta.chunk_count,
        }),
    );

    Ok(IndexSummary {
        kb_id: kb_id.to_string(),
        status: "ready".to_string(),
        index_type: "hnsw".to_string(),
        chunk_count: meta.chunk_count,
        embedding_dim: meta.embedding_dim,
        index_path: meta.index_path,
        skipped,
    })
}

/// 查询索引状态（读 `kb_index_meta` + `kb.index_status`）。
pub async fn get_index_status(pool: &SqlitePool, kb_id: &str) -> Result<IndexSummary, String> {
    let repo = KbRepository::new(pool.clone());
    let meta = repo.get_index_meta(kb_id).await.map_err(|e| e.to_string())?;
    let kb = repo.get_kb(kb_id).await.map_err(|e| e.to_string())?;
    Ok(IndexSummary {
        kb_id: kb_id.to_string(),
        status: kb.index_status.clone(),
        index_type: meta
            .as_ref()
            .map(|m| m.index_type.clone())
            .unwrap_or_else(|| "hnsw".to_string()),
        chunk_count: meta.as_ref().map(|m| m.chunk_count).unwrap_or(0),
        embedding_dim: meta.as_ref().map(|m| m.embedding_dim).unwrap_or(0),
        index_path: meta.and_then(|m| m.index_path),
        skipped: 0,
    })
}

/// HNSW 索引文件目录（`<app_data_dir>/hnsw_indexes`）。
pub fn index_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {e}"))?
        .join("hnsw_indexes");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建索引目录失败: {e}"))?;
    Ok(dir)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
