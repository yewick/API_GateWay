//! 知识库 Tauri command：封装 repository / rag / retriever / processor / importer 层，
//! 供前端 `invoke` 调用。与 HTTP `/api/kb/*` 复用同一套底层逻辑。

use std::sync::Arc;
use tauri::{AppHandle, State};

use sha2::{Digest, Sha256};

use crate::AppState;
use crate::db::repository::Repository;
use crate::services::knowledge::models::*;
use crate::services::knowledge::repository::KbRepository;
use crate::services::knowledge::{
    build_index, embed, get_index_status, import_source, ingest_document,
    parse_document_background, rag, retriever, validate_embedding_config, ImportSummary,
    ProcessOutcome,
};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

#[tauri::command]
pub async fn get_knowledge_bases(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<KbKnowledgeBase>, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_all_kbs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_knowledge_base(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<KbKnowledgeBase, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_kb(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_knowledge_base(
    state: State<'_, Arc<AppState>>,
    input: CreateKbInput,
) -> Result<KbKnowledgeBase, String> {
    if input.name.trim().is_empty() {
        return Err("知识库名称不能为空".to_string());
    }
    let db = Repository::new(state.db.pool.clone());
    validate_embedding_config(
        &db,
        input.embedding_model.as_deref(),
        input.embedding_channel_id.as_deref(),
    )
    .await?;

    let repo = KbRepository::new(state.db.pool.clone());
    repo.create_kb(&input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_knowledge_base(
    state: State<'_, Arc<AppState>>,
    id: String,
    input: UpdateKbInput,
) -> Result<KbKnowledgeBase, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.update_kb(&id, &input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_knowledge_base(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.delete_kb(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ask_knowledge_base(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    question: String,
    model: Option<String>,
    top_k: Option<usize>,
    history: Option<Vec<ConversationMessage>>,
    api_key_id: Option<String>,
) -> Result<RagAnswer, String> {
    let chat_model = model.unwrap_or_else(|| "gpt-4o".to_string());
    let top_k = top_k.unwrap_or(5).max(1);

    // 显式指定密钥（前端在已注册密钥中选取）优先；否则交由内部自动选择
    let api_key = match api_key_id {
        Some(id) => {
            let db = Repository::new(state.db.pool.clone());
            Some(
                db.get_api_key_by_id(&id)
                    .await
                    .map_err(|e| format!("读取 API 密钥失败: {e}"))?,
            )
        }
        None => None,
    };

    rag::ask(
        &state.db.pool,
        &app,
        &kb_id,
        &question,
        &chat_model,
        top_k,
        false,
        history.as_deref(),
        None,
        api_key,
    )
    .await
}

#[tauri::command]
pub async fn get_kb_conversations(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<Vec<KbConversation>, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_conversations(&kb_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_kb_conversations(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<(), String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.delete_conversations(&kb_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_kb_documents(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<Vec<KbDocument>, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_documents(&kb_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_kb_document(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    doc_id: String,
) -> Result<KbDocument, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    let doc = repo.get_document(&doc_id).await.map_err(|e| e.to_string())?;
    if doc.kb_id != kb_id {
        return Err("文档不属于该知识库".to_string());
    }
    Ok(doc)
}

#[tauri::command]
pub async fn get_kb_document_content(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    doc_id: String,
) -> Result<DocumentContent, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    let doc = repo.get_document(&doc_id).await.map_err(|e| e.to_string())?;
    if doc.kb_id != kb_id {
        return Err("文档不属于该知识库".to_string());
    }
    Ok(DocumentContent {
        content: doc.content,
        file_type: doc.file_type,
    })
}

/// 上传文档：前端只传文件路径，Rust 直读字节（不经过 base64），
/// 随后与 HTTP 上传走完全相同的解析/去重/建任务管线。
#[tauri::command]
pub async fn upload_kb_document(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    path: String,
) -> Result<UploadDocumentResult, String> {
    let repo = KbRepository::new(state.db.pool.clone());

    // 1. 读取文件字节
    let bytes = std::fs::read(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    // 2. SHA256 内容哈希（去重）
    let hash = sha256_hex(&bytes);

    // 3. 同一知识库内相同内容跳过
    if let Some(existing) = repo
        .get_document_by_hash(&kb_id, &hash)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(UploadDocumentResult {
            document: existing,
            task_id: String::new(),
            duplicate: true,
        });
    }

    // 4. 先落一条「解析中」文档记录（content 空，后台解析后回填）
    let now = crate::utils::time::now_iso();
    let doc_id = crate::utils::id::new_id();
    let doc = KbDocument {
        id: doc_id.clone(),
        kb_id: kb_id.clone(),
        filename: filename.clone(),
        file_path: Some(path.clone()),
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
        source_path: Some(path),
        doc_meta: "{}".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let doc = repo.create_document(&doc).await.map_err(|e| e.to_string())?;

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
    let task = repo.create_task(&task).await.map_err(|e| e.to_string())?;

    let pool = state.db.pool.clone();
    let kb = kb_id.clone();
    let doc_for_spawn = doc_id;
    let task_for_spawn = task.id.clone();
    tauri::async_runtime::spawn(async move {
        parse_document_background(
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

    Ok(UploadDocumentResult {
        document: doc,
        task_id: task.id,
        duplicate: false,
    })
}

#[tauri::command]
pub async fn ingest_kb_document(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    doc_id: String,
) -> Result<ProcessOutcome, String> {
    ingest_document(&state.db.pool, &kb_id, &doc_id, &app).await
}

#[tauri::command]
pub async fn delete_kb_document(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    doc_id: String,
) -> Result<(), String> {
    let repo = KbRepository::new(state.db.pool.clone());
    let doc = repo.get_document(&doc_id).await.map_err(|e| e.to_string())?;
    if doc.kb_id != kb_id {
        return Err("文档不属于该知识库".to_string());
    }
    repo.delete_chunks_by_doc(&doc_id).await.map_err(|e| e.to_string())?;
    repo.delete_document(&doc_id).await.map_err(|e| e.to_string())?;
    // 只有已入库（有切片）的文档才计入过 doc_count，才需要回减
    if doc.chunk_count > 0 {
        repo.increment_kb_counts(&kb_id, -1, -doc.chunk_count, -doc.token_count)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_kb_stats(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<KbStats, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_kb_stats(&kb_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn build_kb_index(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<IndexSummary, String> {
    build_index(&app, &state.db.pool, &kb_id).await
}

#[tauri::command]
pub async fn get_kb_index(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<IndexSummary, String> {
    get_index_status(&state.db.pool, &kb_id).await
}

#[tauri::command]
pub async fn search_kb(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    query: String,
    top_k: Option<usize>,
    symbol_kind: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("缺少 query 参数".to_string());
    }
    let top_k = top_k.unwrap_or(5).clamp(1, 100);
    let repo = KbRepository::new(state.db.pool.clone());
    let db = Repository::new(state.db.pool.clone());

    // 确定 embedding 模型与指定渠道（kb_id 为空 → 全局默认）
    let (embedding_model, embedding_channel_id) = if kb_id.is_empty() {
        (rag::default_embedding_model(&app), None)
    } else {
        let kb = repo.get_kb(&kb_id).await.map_err(|e| e.to_string())?;
        (
            kb.embedding_model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| rag::default_embedding_model(&app)),
            kb.embedding_channel_id.clone(),
        )
    };

    let vecs = embed(
        &[query.clone()],
        &embedding_model,
        embedding_channel_id.as_deref(),
        &db,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    let query_emb = vecs
        .into_iter()
        .next()
        .ok_or_else(|| "向量化返回空结果".to_string())?;

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
    .map_err(|e| e.to_string())?;

    if let Some(kind) = symbol_kind.as_deref() {
        results = retriever::filter_by_symbol(results, kind);
    }

    Ok(results)
}

#[tauri::command]
pub async fn import_kb_source(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    input: ImportSourceInput,
) -> Result<ImportSummary, String> {
    import_source(&state.db.pool, &kb_id, input, &app).await
}

#[tauri::command]
pub async fn list_kb_sources(
    state: State<'_, Arc<AppState>>,
    kb_id: String,
) -> Result<Vec<KbSource>, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.list_sources(&kb_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_kb_source(
    state: State<'_, Arc<AppState>>,
    _kb_id: String,
    source_id: String,
) -> Result<(), String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.delete_source(&source_id).await.map_err(|e| e.to_string())
}
