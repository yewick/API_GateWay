//! 流式请求的日志守卫。
//!
//! 关键点：
//! - 正常结束时，调用方在发送 `response.completed` / `message_stop` **之前**
//!   调用 `finalize`，避免客户端收到终止事件后立刻断开导致日志/配额丢失。
//! - 客户端中途断开时，`Drop` 会补写一条 499 日志并扣配额。
//! - `finalize` 与 `Drop` 通过 `take()` 抢占，保证同一请求只写一次。

use crate::db::models::RequestLog;
use crate::db::repository::Repository;
use crate::security::SecurityFinding;
use std::sync::{Arc, Mutex};

pub struct PendingLog {
    pub log: RequestLog,
    repo: Arc<Repository>,
    findings: Vec<SecurityFinding>,
    security_action: String,
    started_at: std::time::Instant,
}

impl PendingLog {
    pub fn new(
        log: RequestLog,
        repo: Arc<Repository>,
        findings: Vec<SecurityFinding>,
        security_action: String,
        started_at: std::time::Instant,
    ) -> Self {
        Self {
            log,
            repo,
            findings,
            security_action,
            started_at,
        }
    }

    async fn write(mut self, status: i64, error: Option<String>, response_body: Option<String>) {
        self.log.status_code = status;
        self.log.error_message = error;
        self.log.response_body = response_body;
        self.log.duration_ms = self.started_at.elapsed().as_millis() as i64;
        let id = self.log.id.clone();
        let api_key_id = self.log.api_key_id.clone();
        let total_tokens = self.log.total_tokens;
        let _ = self.repo.create_log(&self.log).await;
        let _ = self
            .repo
            .create_security_findings(&id, &self.findings, &self.security_action)
            .await;
        if total_tokens > 0 {
            if let Some(key_id) = api_key_id {
                let _ = self.repo.increment_quota(&key_id, total_tokens).await;
            }
        }
    }
}

pub struct StreamLogGuard {
    slot: Arc<Mutex<Option<PendingLog>>>,
}

impl StreamLogGuard {
    pub fn new(pending: PendingLog) -> Self {
        Self {
            slot: Arc::new(Mutex::new(Some(pending))),
        }
    }

    /// 更新旁路解析到的 token 用量（中途断开时也能留下可用数据）。
    pub fn update_tokens(&self, prompt: i64, completion: i64, total: i64) {
        if let Ok(mut slot) = self.slot.lock() {
            if let Some(pending) = slot.as_mut() {
                pending.log.prompt_tokens = prompt;
                pending.log.completion_tokens = completion;
                pending.log.total_tokens = total;
            }
        }
    }

    pub fn mark_error(&self, message: String) {
        if let Ok(mut slot) = self.slot.lock() {
            if let Some(pending) = slot.as_mut() {
                pending.log.error_message = Some(message);
            }
        }
    }

    /// 显式落库（正常结束/上游错误）。spawn 后在当前 future 中等待，
    /// 即使此时客户端断开、外层生成器被 drop，写库任务也会继续完成。
    pub async fn finalize(
        &self,
        status: i64,
        error: Option<String>,
        response_body: Option<String>,
    ) {
        let pending = self.slot.lock().ok().and_then(|mut slot| slot.take());
        if let Some(pending) = pending {
            let _ = tokio::spawn(async move {
                pending.write(status, error, response_body).await;
            })
            .await;
        }
    }

    /// 已经落库后清空槽位，Drop 不再重复写。
    pub fn disarm(&self) {
        if let Ok(mut slot) = self.slot.lock() {
            let _ = slot.take();
        }
    }
}

impl Drop for StreamLogGuard {
    fn drop(&mut self) {
        let pending = self.slot.lock().ok().and_then(|mut slot| slot.take());
        let Some(pending) = pending else {
            return;
        };
        let future = async move {
            pending
                .write(
                    499,
                    Some("客户端提前断开（流式未正常结束）".to_string()),
                    None,
                )
                .await;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(future);
        } else {
            tauri::async_runtime::spawn(future);
        }
    }
}
