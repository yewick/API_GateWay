//! 启动后分批补算历史日志的会话字段（幂等，不阻塞启动）。

use crate::db::models::LogBackfillRow;
use crate::db::repository::Repository;
use crate::server::session_identity::{self, Protocol};
use axum::http::HeaderMap;
use serde_json::Value;
use std::sync::Arc;

const BATCH_SIZE: i64 = 500;
const MAX_BATCHES: usize = 40;

fn detect_protocol(body: &Value, mode: &str) -> Protocol {
    if body.get("input").is_some() && body.get("messages").is_none() {
        return Protocol::Responses;
    }
    if body.get("messages").is_some() {
        if body.get("system").is_some()
            || body.get("metadata").is_some()
            || body
                .get("tools")
                .and_then(Value::as_array)
                .is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|tool| tool.get("input_schema").is_some())
                })
        {
            return Protocol::Messages;
        }
        return Protocol::Chat;
    }
    match mode {
        "responses" => Protocol::Responses,
        "messages" => Protocol::Messages,
        _ => Protocol::Chat,
    }
}

async fn process_batch(repo: &Repository, rows: Vec<LogBackfillRow>) {
    for row in rows {
        let Ok(body) = serde_json::from_str::<Value>(&row.request_body) else {
            continue;
        };
        let protocol = detect_protocol(&body, &row.mode);
        let meta = session_identity::extract(
            &HeaderMap::new(),
            &body,
            protocol,
            row.api_key_id.as_deref().unwrap_or("unknown"),
        );
        let _ = repo
            .update_log_session(
                &row.id,
                meta.session_key.as_deref(),
                meta.client_session_id.as_deref(),
                &meta.session_source,
                meta.session_client.as_deref(),
                meta.session_title.as_deref(),
                &meta.request_kind,
                meta.history_len,
            )
            .await;
    }
}

pub async fn backfill_sessions(repo: Arc<Repository>) {
    for _ in 0..MAX_BATCHES {
        let rows = match repo.list_logs_missing_session(BATCH_SIZE).await {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!("补算历史日志会话失败: {err}");
                return;
            }
        };
        let count = rows.len();
        if count == 0 {
            return;
        }
        process_batch(&repo, rows).await;
        if count < BATCH_SIZE as usize {
            return;
        }
    }
}
