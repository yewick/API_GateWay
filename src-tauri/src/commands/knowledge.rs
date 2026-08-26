//! 知识库 Tauri command：封装 repository / rag 层，供前端 `invoke` 调用。

use std::sync::Arc;
use tauri::State;

use crate::AppState;
use crate::services::knowledge::models::*;
use crate::services::knowledge::rag;
use crate::services::knowledge::repository::KbRepository;

#[tauri::command]
pub async fn get_knowledge_bases(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<KbKnowledgeBase>, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.get_all_kbs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_knowledge_base(
    state: State<'_, Arc<AppState>>,
    input: CreateKbInput,
) -> Result<KbKnowledgeBase, String> {
    let repo = KbRepository::new(state.db.pool.clone());
    repo.create_kb(&input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ask_knowledge_base(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    kb_id: String,
    question: String,
    model: Option<String>,
    top_k: Option<usize>,
) -> Result<RagAnswer, String> {
    let chat_model = model.unwrap_or_else(|| "gpt-4o".to_string());
    let top_k = top_k.unwrap_or(5).max(1);
    rag::ask(
        &state.db.pool,
        &app,
        &kb_id,
        &question,
        &chat_model,
        top_k,
        false,
        None,
        None,
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
