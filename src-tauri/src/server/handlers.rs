use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Json, IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

// 临时调试：写错误日志到文件
fn debug_log(msg: &str) {
    let path = std::path::PathBuf::from("/tmp/yeapi_debug.log");
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    let line = format!("{} {}\n", ts, msg);
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

use super::router::SharedState;
use crate::adaptor::{get_adaptor, ProxyRequest};
use crate::core::dispatcher::Dispatcher;
use crate::core::proxy;
use crate::db::models::RequestLog;
use crate::db::repository::Repository;
use crate::protocol;
use crate::protocol::anthropic::AnthropicStreamConverter;
use crate::protocol::responses::ResponsesStreamConverter;
use crate::security::{self, SecurityAction};
use crate::utils;

// ---------- 统一错误响应（OpenAI 兼容格式） ----------

fn error_response(code: u16, msg: &str) -> Response {
    let body = serde_json::json!({
        "error": { "message": msg, "type": "upstream_error", "code": code }
    });
    (
        StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY),
        Json(body),
    )
        .into_response()
}

fn not_implemented(endpoint: &str) -> Response {
    let body = serde_json::json!({
        "error": { "message": format!("{} not implemented yet", endpoint), "type": "not_implemented" }
    });
    (StatusCode::NOT_IMPLEMENTED, Json(body)).into_response()
}

// ---------- Anthropic 错误响应（type: error 包装） ----------

fn anthropic_error_response(code: u16, msg: &str) -> Response {
    let body = serde_json::json!({
        "type": "error",
        "error": { "type": "api_error", "message": msg }
    });
    (
        StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY),
        Json(body),
    )
        .into_response()
}

// ---------- 鉴权 + 配额（供 /v1/messages、/v1/responses 复用） ----------

struct AuthContext {
    api_key_id: String,
    api_key_name: String,
    repo: Arc<Repository>,
}

fn key_is_expired(expires_at: &str) -> bool {
    if expires_at.is_empty() {
        return false;
    }
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expiry| chrono::Utc::now() > expiry)
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%dT%H:%M:%S%.3fZ")
                .map(|d| chrono::Utc::now() > d.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(expires_at, "%Y-%m-%d")
                .map(|d| chrono::Utc::now() > d.and_hms_opt(23, 59, 59).unwrap().and_utc())
        })
        .unwrap_or(false)
}

async fn authenticate_and_check_quota(
    shared: &SharedState,
    headers: &HeaderMap,
    max_tokens: i64,
) -> Result<AuthContext, (u16, String)> {
    let api_key = match protocol::extract_api_key(headers) {
        Some(k) => k,
        None => return Err((401, "Missing API key".to_string())),
    };

    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let key_record = match repo.get_api_key_by_key(&api_key).await {
        Ok(k) => k,
        Err(_) => return Err((401, "Invalid API key".to_string())),
    };

    if let Some(ref expires_at) = key_record.expires_at {
        if key_is_expired(expires_at) {
            return Err((401, "API key has expired".to_string()));
        }
    }

    if key_record.quota_limit > 0 {
        let remaining = key_record.quota_limit - key_record.quota_used;
        if remaining <= 0 {
            return Err((429, "Quota exceeded".to_string()));
        }
        let projected = key_record.quota_used.saturating_add(max_tokens);
        if projected > key_record.quota_limit {
            return Err((
                429,
                format!("Quota exceeded (remaining: {}, max_tokens: {})", remaining, max_tokens),
            ));
        }
        if remaining < 2000 {
            return Err((
                429,
                format!("Quota nearly exhausted — remaining {} tokens, at least 2000 required", remaining),
            ));
        }
    }

    Ok(AuthContext {
        api_key_id: key_record.id,
        api_key_name: key_record.name,
        repo,
    })
}

// ---------- /v1/chat/completions ----------

pub async fn handle_chat_completions(
    State(shared): State<SharedState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. 解析 JSON
    let body_str = String::from_utf8_lossy(&body);
    let json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(j) => j,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response(),
    };

    let is_stream = json.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    // 2. API Key 鉴权
    let auth_header = headers.get("authorization").and_then(|h| h.to_str().ok()).unwrap_or("");
    let api_key = auth_header.strip_prefix("Bearer ").unwrap_or("").trim();

    if api_key.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Missing API key").into_response();
    }

    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let key_record = match repo.get_api_key_by_key(api_key).await {
        Ok(k) => k,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid API key").into_response(),
    };

    // 2.5 过期检查
    if let Some(ref expires_at) = key_record.expires_at {
        if !expires_at.is_empty() {
            let expired = chrono::DateTime::parse_from_rfc3339(expires_at)
                .map(|expiry| chrono::Utc::now() > expiry)
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%dT%H:%M:%S%.3fZ")
                        .map(|d| chrono::Utc::now() > d.and_utc())
                })
                .or_else(|_| {
                    chrono::NaiveDate::parse_from_str(expires_at, "%Y-%m-%d")
                        .map(|d| {
                            chrono::Utc::now()
                                > d.and_hms_opt(23, 59, 59).unwrap().and_utc()
                        })
                })
                .unwrap_or(false);
            if expired {
                return error_response(401, "API key has expired");
            }
        }
    }

    // 3. 配额检查（带 max_tokens 预估缓冲）
    if key_record.quota_limit > 0 {
        let remaining = key_record.quota_limit - key_record.quota_used;
        if remaining <= 0 {
            return error_response(429, "Quota exceeded");
        }
        // 用请求中的 max_tokens 做最低限度预估
        let max_tokens = json
            .get("max_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let projected = key_record.quota_used.saturating_add(max_tokens);
        if projected > key_record.quota_limit {
            return error_response(
                429,
                &format!(
                    "Quota exceeded (remaining: {}, max_tokens: {})",
                    remaining, max_tokens
                ),
            );
        }
        if remaining < 2000 {
            return error_response(
                429,
                &format!(
                    "Quota nearly exhausted — remaining {} tokens, at least 2000 required",
                    remaining
                ),
            );
        }
    }

    // 4. 保存原始请求体（日志用）
    let request_body_str = serde_json::to_string(&json).unwrap_or_default();

    // 4.5 追踪 ID（下游可注入）
    let trace_id = protocol::extract_trace_id(&headers);

    // 5. 分流：流式 vs 非流式
    if is_stream {
        handle_stream(shared, json, key_record.id, key_record.name, request_body_str, trace_id).await
    } else {
        match proxy::handle_request(&repo, &shared.app, &key_record.id, &key_record.name,
                                     json, false, Some(request_body_str), trace_id).await {
            Ok(result) => (StatusCode::OK, Json(result.body)).into_response(),
            Err((code, msg)) => error_response(code, &msg),
        }
    }
}

// ---------- 流式转发 ----------

fn parse_usage_from_chunk(text: &str) -> Option<(i64, i64, i64)> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") {
            continue;
        }
        let data_str = trimmed.trim_start_matches("data:").trim();
        if data_str == "[DONE]" || data_str.is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data_str) {
            if let Some(usage) = json.get("usage") {
                let prompt = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let completion = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let total = usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                if total > 0 || prompt > 0 || completion > 0 {
                    return Some((prompt, completion, total));
                }
            }
        }
    }
    None
}

async fn handle_stream(
    shared: SharedState,
    json: serde_json::Value,
    api_key_id: String,
    api_key_name: String,
    request_body: String,
    trace_id: Option<String>,
) -> Response {
    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let model = json
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // 安全扫描（与非流式路径一致）
    let security_settings = security::get_security_settings(&shared.app);
    // 加载自定义规则
    let custom_rules = repo
        .get_enabled_custom_rules()
        .await
        .unwrap_or_default();
    // 加载被禁用的内置规则
    let disabled_builtin: HashSet<String> = repo
        .get_all_builtin_rules()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.enabled == 0)
        .map(|r| r.rule_id)
        .collect();
    let security_result =
        security::scan_request(&json, &security_settings, Some(&custom_rules), &disabled_builtin);
    if matches!(security_result.action, SecurityAction::Block) {
        let log = RequestLog {
            id: utils::id::new_id(),
            seq: None,
            api_key_id: Some(api_key_id.clone()),
            api_key_name: Some(api_key_name.clone()),
            channel_id: None,
            channel_name: None,
            model: model.clone(),
            upstream_model: None,
            mode: "chat".to_string(),
            status_code: 451,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            duration_ms: 0,
            error_message: security_result.blocked_reason.clone(),
            is_stream: 1,
            is_retry: 0,
            created_at: utils::time::now_iso(),
            request_body: Some(request_body.clone()),
            forward_body: None,
            response_choices: None,
            trace_id: trace_id.clone(),
            risk_level: security_result.risk_level.as_str().to_string(),
            risk_score: security_result.risk_score as i64,
            risk_summary: Some(security_result.summary.clone()),
            security_action: security_result.action.as_str().to_string(),
            sanitized: 0,
            blocked_reason: security_result.blocked_reason.clone(),
        };
        let log_id = log.id.clone();
        let _ = repo.create_log(&log).await;
        let _ = repo.create_security_findings(
            &log_id,
            &security_result.findings,
            security_result.action.as_str(),
        ).await;
        return error_response(451, &security_result.summary);
    }

    // 渠道调度
    let channels = match repo.get_enabled_channels().await {
        Ok(c) => c,
        Err(e) => return error_response(500, &format!("DB error: {}", e)),
    };
    let selected_channels = Dispatcher::select_channels(&channels, &model);
    if selected_channels.is_empty() {
        return error_response(503, &format!("No channel available for model: {}", model));
    }

    // 重试配置
    let (retry_enabled, retry_times) = proxy::get_retry_settings(&shared.app);
    let max_attempts = if retry_enabled {
        (retry_times.max(0) as usize + 1).min(selected_channels.len())
    } else {
        1
    };

    let mut last_error: Option<String> = None;

    debug_log(&format!("handle_stream: model={} body_size={} channels={}",
        model, request_body.len(), selected_channels.len()));

    for (attempt, channel) in selected_channels.into_iter().take(max_attempts).enumerate() {
        let config = Dispatcher::channel_to_config(&channel);
        let adaptor = get_adaptor(&channel.channel_type);
        let request = ProxyRequest {
            model: model.clone(),
            body: json.clone(),
            stream: true,
        };

        debug_log(&format!("attempt={}: channel={} type={} url={}",
            attempt, channel.name, channel.channel_type, config.base_url));

        match adaptor.forward_stream(&request, &config).await {
            Ok(resp) => {
                let status = resp.status();
                debug_log(&format!("upstream status={}", status));
                if !status.is_success() {
                    // 上游返回错误（非 2xx），读错误体，尝试下一个渠道
                    let body_str = resp.text().await.unwrap_or_default();
                    debug_log(&format!("upstream error body: {}",
                        &body_str[..body_str.len().min(2000)]));
                    last_error = Some(format!("{}: {}", channel.name, body_str));
                    continue;
                }

                let start = Instant::now();
                let is_retry = if attempt > 0 { 1 } else { 0 };

                // 克隆日志所需数据（闭包要 move 进流中）
                let repo_clone = repo.clone();
                let api_key_id_c = api_key_id.clone();
                let api_key_name_c = api_key_name.clone();
                let model_c = model.clone();
                let request_body_c = request_body.clone();
                let channel_name_c = channel.name.clone();
                let channel_id_c = channel.id.clone();
                let trace_id_c = trace_id.clone();

                // ── 克隆扫描结果（闭包要 move 进流中）
                let security_result_clone = security_result.clone();

                // ── 核心：字节透传 + 旁路解析 ──────────────────
                let upstream_stream = resp.bytes_stream();

                let passthrough_stream = async_stream::stream! {
                    tokio::pin!(upstream_stream);

                    let mut usage_prompt: i64 = 0;
                    let mut usage_completion: i64 = 0;
                    let mut usage_total: i64 = 0;
                    let mut had_error = false;

                    // 逐 chunk 透传
                    while let Some(chunk_result) = upstream_stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                // 旁路解析 usage（不影响转发）
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    if let Some((p, c, t)) = parse_usage_from_chunk(text) {
                                        usage_prompt = p;
                                        usage_completion = c;
                                        usage_total = t;
                                    }
                                }
                                yield Ok::<_, std::io::Error>(bytes);
                            }
                            Err(e) => {
                                // 流中断：补发错误 chunk + [DONE]，优雅收尾
                                had_error = true;
                                let err_chunk = format!(
                                    "data: {{\"error\":{{\"message\":\"Stream connection interrupted: {}\",\"type\":\"server_error\"}}}}\n\n", e
                                );
                                yield Ok::<_, std::io::Error>(err_chunk.into_bytes().into());
                                yield Ok::<_, std::io::Error>(Bytes::from_static(b"data: [DONE]\n\n"));
                                break;
                            }
                        }
                    }

                    // ── 流结束后：统一写日志 + 扣配额 ──────────
                    let log = RequestLog {
                        id: utils::id::new_id(),
                        seq: None,
                        api_key_id: Some(api_key_id_c.clone()),
                        api_key_name: Some(api_key_name_c),
                        channel_id: Some(channel_id_c),
                        channel_name: Some(channel_name_c),
                        model: model_c.clone(),
                        upstream_model: Some(model_c),
                        mode: "chat".to_string(),
                        status_code: if had_error { 502 } else { 200 },
                        prompt_tokens: usage_prompt,
                        completion_tokens: usage_completion,
                        total_tokens: usage_total,
                        duration_ms: start.elapsed().as_millis() as i64,
                        error_message: if had_error {
                            Some("Stream connection interrupted".to_string())
                        } else {
                            None
                        },
                        is_stream: 1,
                        is_retry,
                        created_at: utils::time::now_iso(),
                        request_body: Some(request_body_c),
                        forward_body: None,
                        response_choices: None,
                        trace_id: trace_id_c,
                        risk_level: security_result_clone.risk_level.as_str().to_string(),
                        risk_score: security_result_clone.risk_score as i64,
                        risk_summary: if security_result_clone.summary.is_empty() {
                            None
                        } else {
                            Some(security_result_clone.summary.clone())
                        },
                        security_action: security_result_clone.action.as_str().to_string(),
                        sanitized: if security_result_clone.sanitized { 1 } else { 0 },
                        blocked_reason: security_result_clone.blocked_reason.clone(),
                    };
                    let log_id = log.id.clone();
                    let _ = repo_clone.create_log(&log).await;
                    let _ = repo_clone
                        .create_security_findings(
                            &log_id,
                            &security_result_clone.findings,
                            security_result_clone.action.as_str(),
                        )
                        .await;
                    if usage_total > 0 {
                        let _ = repo_clone.increment_quota(&api_key_id_c, usage_total).await;
                    }
                };

                // 返回 SSE 响应
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header(header::CONNECTION, "keep-alive")
                    .body(Body::from_stream(passthrough_stream))
                    .unwrap();
            }
            Err(e) => {
                // 连接失败：记日志，尝试下一个渠道
                debug_log(&format!("forward_stream error: {:?}", e));
                last_error = Some(format!("{}: {}", channel.name, e));
            }
        }
    }

    // 所有渠道失败
    debug_log(&format!("ALL CHANNELS FAILED: {:?}", last_error));
    error_response(502, &format!("All stream channels failed: {:?}", last_error))
}

// ---------- 多协议流式转换（Anthropic / Responses 共用骨架） ----------

trait StreamConverter {
    fn new(model: &str) -> Self;
    fn push(&mut self, bytes: &[u8]) -> Vec<String>;
    fn finish(&mut self) -> Vec<String>;
}

impl StreamConverter for AnthropicStreamConverter {
    fn new(model: &str) -> Self {
        AnthropicStreamConverter::new(model)
    }
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        AnthropicStreamConverter::push(self, bytes)
    }
    fn finish(&mut self) -> Vec<String> {
        AnthropicStreamConverter::finish(self)
    }
}

impl StreamConverter for ResponsesStreamConverter {
    fn new(model: &str) -> Self {
        ResponsesStreamConverter::new(model)
    }
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        ResponsesStreamConverter::push(self, bytes)
    }
    fn finish(&mut self) -> Vec<String> {
        ResponsesStreamConverter::finish(self)
    }
}

async fn proxy_stream<C: StreamConverter + Send>(
    shared: SharedState,
    auth: AuthContext,
    openai_body: serde_json::Value,
    model: String,
    request_body: String,
    trace_id: Option<String>,
    mode: &'static str,
) -> Response {
    let repo = auth.repo.clone();

    // 安全扫描（与非流式路径一致）
    let security_settings = security::get_security_settings(&shared.app);
    let custom_rules = repo.get_enabled_custom_rules().await.unwrap_or_default();
    let disabled_builtin: HashSet<String> = repo
        .get_all_builtin_rules()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.enabled == 0)
        .map(|r| r.rule_id)
        .collect();
    let security_result = security::scan_request(&openai_body, &security_settings, Some(&custom_rules), &disabled_builtin);
    if matches!(security_result.action, SecurityAction::Block) {
        let log = RequestLog {
            id: utils::id::new_id(),
            seq: None,
            api_key_id: Some(auth.api_key_id.clone()),
            api_key_name: Some(auth.api_key_name.clone()),
            channel_id: None,
            channel_name: None,
            model: model.clone(),
            upstream_model: None,
            mode: mode.to_string(),
            status_code: 451,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            duration_ms: 0,
            error_message: security_result.blocked_reason.clone(),
            is_stream: 1,
            is_retry: 0,
            created_at: utils::time::now_iso(),
            request_body: Some(request_body.clone()),
            forward_body: None,
            response_choices: None,
            trace_id: trace_id.clone(),
            risk_level: security_result.risk_level.as_str().to_string(),
            risk_score: security_result.risk_score as i64,
            risk_summary: Some(security_result.summary.clone()),
            security_action: security_result.action.as_str().to_string(),
            sanitized: 0,
            blocked_reason: security_result.blocked_reason.clone(),
        };
        let log_id = log.id.clone();
        let _ = repo.create_log(&log).await;
        let _ = repo
            .create_security_findings(&log_id, &security_result.findings, security_result.action.as_str())
            .await;
        return error_response(451, &security_result.summary);
    }

    // 渠道调度
    let channels = match repo.get_enabled_channels().await {
        Ok(c) => c,
        Err(e) => return error_response(500, &format!("DB error: {}", e)),
    };
    let selected_channels = Dispatcher::select_channels(&channels, &model);
    if selected_channels.is_empty() {
        return error_response(503, &format!("No channel available for model: {}", model));
    }

    let (retry_enabled, retry_times) = proxy::get_retry_settings(&shared.app);
    let max_attempts = if retry_enabled {
        (retry_times.max(0) as usize + 1).min(selected_channels.len())
    } else {
        1
    };

    let mut last_error: Option<String> = None;

    for (attempt, channel) in selected_channels.into_iter().take(max_attempts).enumerate() {
        let config = Dispatcher::channel_to_config(&channel);
        let adaptor = get_adaptor(&channel.channel_type);
        let request = ProxyRequest {
            model: model.clone(),
            body: openai_body.clone(),
            stream: true,
        };

        match adaptor.forward_stream(&request, &config).await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body_str = resp.text().await.unwrap_or_default();
                    last_error = Some(format!("{}: {}", channel.name, body_str));
                    continue;
                }

                let start = Instant::now();
                let is_retry = if attempt > 0 { 1 } else { 0 };

                let repo_clone = repo.clone();
                let api_key_id_c = auth.api_key_id.clone();
                let api_key_name_c = auth.api_key_name.clone();
                let model_c = model.clone();
                let request_body_c = request_body.clone();
                let channel_name_c = channel.name.clone();
                let channel_id_c = channel.id.clone();
                let trace_id_c = trace_id.clone();
                let security_result_clone = security_result.clone();

                let upstream_stream = resp.bytes_stream();

                let converted_stream = async_stream::stream! {
                    tokio::pin!(upstream_stream);

                    let mut converter = C::new(&model_c);
                    let mut usage_prompt: i64 = 0;
                    let mut usage_completion: i64 = 0;
                    let mut usage_total: i64 = 0;
                    let mut had_error = false;

                    while let Some(chunk_result) = upstream_stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                // 旁路解析 usage（不影响转换）
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    if let Some((p, c, t)) = protocol::parse_usage_from_sse_chunk(text) {
                                        usage_prompt = p;
                                        usage_completion = c;
                                        usage_total = t;
                                    }
                                }
                                for ev in converter.push(&bytes) {
                                    yield Ok::<_, std::io::Error>(Bytes::from(ev));
                                }
                            }
                            Err(_e) => {
                                had_error = true;
                                for ev in converter.finish() {
                                    yield Ok::<_, std::io::Error>(Bytes::from(ev));
                                }
                                break;
                            }
                        }
                    }

                    // 流结束：补尾事件
                    for ev in converter.finish() {
                        yield Ok::<_, std::io::Error>(Bytes::from(ev));
                    }

                    // 写日志 + 扣配额
                    let log = RequestLog {
                        id: utils::id::new_id(),
                        seq: None,
                        api_key_id: Some(api_key_id_c.clone()),
                        api_key_name: Some(api_key_name_c),
                        channel_id: Some(channel_id_c),
                        channel_name: Some(channel_name_c),
                        model: model_c.clone(),
                        upstream_model: Some(model_c),
                        mode: mode.to_string(),
                        status_code: if had_error { 502 } else { 200 },
                        prompt_tokens: usage_prompt,
                        completion_tokens: usage_completion,
                        total_tokens: usage_total,
                        duration_ms: start.elapsed().as_millis() as i64,
                        error_message: if had_error {
                            Some("Stream connection interrupted".to_string())
                        } else {
                            None
                        },
                        is_stream: 1,
                        is_retry,
                        created_at: utils::time::now_iso(),
                        request_body: Some(request_body_c),
                        forward_body: None,
                        response_choices: None,
                        trace_id: trace_id_c,
                        risk_level: security_result_clone.risk_level.as_str().to_string(),
                        risk_score: security_result_clone.risk_score as i64,
                        risk_summary: if security_result_clone.summary.is_empty() {
                            None
                        } else {
                            Some(security_result_clone.summary.clone())
                        },
                        security_action: security_result_clone.action.as_str().to_string(),
                        sanitized: if security_result_clone.sanitized { 1 } else { 0 },
                        blocked_reason: security_result_clone.blocked_reason.clone(),
                    };
                    let log_id = log.id.clone();
                    let _ = repo_clone.create_log(&log).await;
                    let _ = repo_clone
                        .create_security_findings(&log_id, &security_result_clone.findings, security_result_clone.action.as_str())
                        .await;
                    if usage_total > 0 {
                        let _ = repo_clone.increment_quota(&api_key_id_c, usage_total).await;
                    }
                };

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header(header::CONNECTION, "keep-alive")
                    .body(Body::from_stream(converted_stream))
                    .unwrap();
            }
            Err(e) => {
                last_error = Some(format!("{}: {}", channel.name, e));
            }
        }
    }

    error_response(502, &format!("All stream channels failed: {:?}", last_error))
}

// ---------- /v1/messages (Anthropic Messages API) ----------

pub async fn handle_messages(
    State(shared): State<SharedState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let body_str = String::from_utf8_lossy(&body);
    let json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(j) => j,
        Err(e) => return anthropic_error_response(400, &format!("Invalid JSON: {}", e)),
    };

    // 转换 Anthropic 请求为 OpenAI 格式
    let openai_body = protocol::anthropic_to_openai(&json);
    let model = openai_body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let is_stream = openai_body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let max_tokens = openai_body.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(0);

    // 鉴权 + 配额
    let auth = match authenticate_and_check_quota(&shared, &headers, max_tokens).await {
        Ok(a) => a,
        Err((code, msg)) => return anthropic_error_response(code, &msg),
    };

    let request_body_str = serde_json::to_string(&json).unwrap_or_default();
    let trace_id = protocol::extract_trace_id(&headers);

    if is_stream {
        handle_messages_stream(shared, auth, openai_body, model, request_body_str, trace_id).await
    } else {
        match proxy::handle_request(&auth.repo, &shared.app, &auth.api_key_id, &auth.api_key_name,
                                     openai_body, false, Some(request_body_str), trace_id).await {
            Ok(result) => {
                let anthropic_resp = protocol::openai_to_anthropic(&result.body, &model);
                (StatusCode::from_u16(result.status).unwrap_or(StatusCode::OK), Json(anthropic_resp)).into_response()
            }
            Err((code, msg)) => anthropic_error_response(code, &msg),
        }
    }
}

async fn handle_messages_stream(
    shared: SharedState,
    auth: AuthContext,
    openai_body: serde_json::Value,
    model: String,
    request_body: String,
    trace_id: Option<String>,
) -> Response {
    proxy_stream::<AnthropicStreamConverter>(shared, auth, openai_body, model, request_body, trace_id, "messages").await
}

// ---------- /v1/responses (OpenAI Responses API) ----------

pub async fn handle_responses(
    State(shared): State<SharedState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let body_str = String::from_utf8_lossy(&body);
    let json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(j) => j,
        Err(e) => return error_response(400, &format!("Invalid JSON: {}", e)),
    };

    // 转换 Responses 请求为 OpenAI 格式
    let openai_body = protocol::responses_to_openai(&json);
    let model = openai_body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let is_stream = openai_body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let max_tokens = openai_body.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(0);

    // 鉴权 + 配额
    let auth = match authenticate_and_check_quota(&shared, &headers, max_tokens).await {
        Ok(a) => a,
        Err((code, msg)) => return error_response(code, &msg),
    };

    let request_body_str = serde_json::to_string(&json).unwrap_or_default();
    let trace_id = protocol::extract_trace_id(&headers);

    if is_stream {
        handle_responses_stream(shared, auth, openai_body, model, request_body_str, trace_id).await
    } else {
        match proxy::handle_request(&auth.repo, &shared.app, &auth.api_key_id, &auth.api_key_name,
                                     openai_body, false, Some(request_body_str), trace_id).await {
            Ok(result) => {
                let responses_resp = protocol::openai_to_responses(&result.body, &model);
                (StatusCode::from_u16(result.status).unwrap_or(StatusCode::OK), Json(responses_resp)).into_response()
            }
            Err((code, msg)) => error_response(code, &msg),
        }
    }
}

async fn handle_responses_stream(
    shared: SharedState,
    auth: AuthContext,
    openai_body: serde_json::Value,
    model: String,
    request_body: String,
    trace_id: Option<String>,
) -> Response {
    proxy_stream::<ResponsesStreamConverter>(shared, auth, openai_body, model, request_body, trace_id, "responses").await
}

// ---------- /v1/models ----------

pub async fn handle_list_models(State(shared): State<SharedState>) -> Response {
    let repo = Repository::new(shared.state.db.pool.clone());
    match repo.get_enabled_channels().await {
        Ok(channels) => {
            let mut models: Vec<serde_json::Value> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for ch in &channels {
                // 渠道声明的模型
                let ch_models: Vec<String> = serde_json::from_str(&ch.models).unwrap_or_default();
                for m in ch_models {
                    if seen.insert(m.clone()) {
                        models.push(serde_json::json!({
                            "id": m, "object": "model",
                            "created": chrono::Utc::now().timestamp(),
                            "owned_by": ch.channel_type,
                        }));
                    }
                }
                // 映射名也暴露（下游看到的名义模型）
                let mapping: serde_json::Value = serde_json::from_str(&ch.model_mapping)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if let Some(obj) = mapping.as_object() {
                    for key in obj.keys() {
                        if seen.insert(key.clone()) {
                            models.push(serde_json::json!({
                                "id": key, "object": "model",
                                "created": chrono::Utc::now().timestamp(),
                                "owned_by": ch.channel_type,
                            }));
                        }
                    }
                }
            }
            Json(serde_json::json!({ "object": "list", "data": models })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {}", e)).into_response(),
    }
}

// ---------- /health ----------

pub async fn handle_health(State(shared): State<SharedState>) -> Response {
    let port = shared.state.server_port.read().await.clone();
    let running = shared.state.server_running.load(std::sync::atomic::Ordering::SeqCst);
    Json(serde_json::json!({
        "status": "ok",
        "running": running,
        "port": port,
        "url": format!("http://127.0.0.1:{}", port),
    })).into_response()
}

// ---------- 占位端点（router 已注册，功能后续实现） ----------

pub async fn handle_completions() -> Response {
    not_implemented("/v1/completions")
}

pub async fn handle_embeddings() -> Response {
    not_implemented("/v1/embeddings")
}

pub async fn handle_images() -> Response {
    not_implemented("/v1/images/generations")
}

pub async fn handle_audio_transcriptions() -> Response {
    not_implemented("/v1/audio/transcriptions")
}

pub async fn handle_audio_speech() -> Response {
    not_implemented("/v1/audio/speech")
}
