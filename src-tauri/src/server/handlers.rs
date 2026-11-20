use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use super::auth::key_is_expired;
use super::quota;
use super::router::SharedState;
use super::session_identity::{self, Protocol, SessionMeta};
use super::stream_log::{PendingLog, StreamLogGuard};
use crate::adaptor::{get_adaptor, ProxyRequest};
use crate::core::dispatcher::Dispatcher;
use crate::core::proxy;
use crate::db::models::{Channel, RequestLog};
use crate::db::repository::Repository;
use crate::protocol;
use crate::protocol::anthropic::AnthropicStreamConverter;
use crate::protocol::chat_stream::ChatStreamAggregator;
use crate::protocol::responses::ResponsesStreamConverter;
use crate::security::{self, SecurityAction};
use crate::utils;

// ---------- 统一错误响应（OpenAI 兼容格式） ----------

/// 启用渠道可接受的模型并集：渠道 `models` ∪ `model_mapping` 的 key（对外别名）。
/// 去重，保留首次出现的渠道类型作为 `owned_by`。
pub fn collect_models(channels: &[crate::db::models::Channel]) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for channel in channels {
        let declared: Vec<String> = serde_json::from_str(&channel.models).unwrap_or_default();
        for model in declared {
            if seen.insert(model.clone()) {
                models.push((model, channel.channel_type.clone()));
            }
        }
        let mapping: serde_json::Value =
            serde_json::from_str(&channel.model_mapping).unwrap_or_default();
        if let Some(object) = mapping.as_object() {
            for alias in object.keys() {
                if seen.insert(alias.clone()) {
                    models.push((alias.clone(), channel.channel_type.clone()));
                }
            }
        }
    }
    models
}

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
    /// 限额 Key 的输出预算；`None` 表示不限量。
    quota: Option<quota::QuotaBudget>,
    session: SessionMeta,
}

async fn authenticate_and_check_quota(
    shared: &SharedState,
    headers: &HeaderMap,
    model: &str,
    body: &serde_json::Value,
    protocol: Protocol,
    mode: &'static str,
    raw_body: &serde_json::Value,
    request_body: Option<&str>,
    requested_max_tokens: i64,
) -> Result<AuthContext, (u16, String)> {
    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let anonymous_session = session_identity::extract(headers, raw_body, protocol, "unknown");

    let api_key = match protocol::extract_api_key(headers) {
        Some(k) => k,
        None => {
            write_failed_log(
                &repo,
                None,
                None,
                model,
                mode,
                401,
                "Missing API key",
                request_body,
                &anonymous_session,
                false,
            )
            .await;
            return Err((401, "Missing API key".to_string()));
        }
    };

    let key_record = match repo.get_api_key_by_key(&api_key).await {
        Ok(k) => k,
        Err(_) => {
            write_failed_log(
                &repo,
                None,
                None,
                model,
                mode,
                401,
                "Invalid API key",
                request_body,
                &anonymous_session,
                false,
            )
            .await;
            return Err((401, "Invalid API key".to_string()));
        }
    };

    let session = session_identity::extract(headers, raw_body, protocol, &key_record.id);

    if let Some(ref expires_at) = key_record.expires_at {
        if key_is_expired(expires_at) {
            write_failed_log(
                &repo,
                Some(&key_record.id),
                Some(&key_record.name),
                model,
                mode,
                401,
                "API key has expired",
                request_body,
                &session,
                false,
            )
            .await;
            return Err((401, "API key has expired".to_string()));
        }
    }

    // 配额预检：估算输入 + 输出预算，而不是把 max_tokens 当成预计消耗。
    let estimated_input = quota::estimate_input_tokens(body);
    let remaining = quota::remaining(key_record.quota_limit, key_record.quota_used);
    let budget = match quota::decide_quota(remaining, estimated_input, requested_max_tokens) {
        Ok(budget) => budget,
        Err(message) => {
            write_failed_log(
                &repo,
                Some(&key_record.id),
                Some(&key_record.name),
                model,
                mode,
                429,
                &message,
                request_body,
                &session,
                false,
            )
            .await;
            return Err((429, message));
        }
    };

    Ok(AuthContext {
        api_key_id: key_record.id,
        api_key_name: key_record.name,
        repo,
        quota: remaining.map(|_| budget),
        session,
    })
}

/// 写一条「请求到达网关但被拒」的失败日志（401/429/503 等），便于在日志页追踪失败原因。
/// 失败不影响响应（静默吞错），与成功路径一致。
#[allow(clippy::too_many_arguments)]
async fn write_failed_log(
    repo: &Repository,
    api_key_id: Option<&str>,
    api_key_name: Option<&str>,
    model: &str,
    mode: &str,
    status_code: u16,
    error_message: &str,
    request_body: Option<&str>,
    session: &SessionMeta,
    is_stream: bool,
) {
    let mut log = RequestLog {
        id: utils::id::new_id(),
        seq: None,
        api_key_id: api_key_id.map(|s| s.to_string()),
        api_key_name: api_key_name.map(|s| s.to_string()),
        channel_id: None,
        channel_name: None,
        model: model.to_string(),
        upstream_model: None,
        mode: mode.to_string(),
        status_code: status_code as i64,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        duration_ms: 0,
        error_message: Some(error_message.to_string()),
        is_stream: if is_stream { 1 } else { 0 },
        is_retry: 0,
        created_at: utils::time::now_iso(),
        request_body: request_body.map(|s| s.to_string()),
        forward_body: None,
        response_choices: None,
        trace_id: None,
        risk_level: "none".to_string(),
        risk_score: 0,
        risk_summary: None,
        security_action: "none".to_string(),
        sanitized: 0,
        blocked_reason: None,
        response_body: None,
        session_key: None,
        client_session_id: None,
        session_source: "none".to_string(),
        session_client: None,
        session_title: None,
        request_kind: "turn".to_string(),
        history_len: None,
    };
    session.apply(&mut log);
    let _ = repo.create_log(&log).await;
}

/// 流式渠道级别的失败日志（无渠道 / 上游非 2xx / 连接失败）。
#[allow(clippy::too_many_arguments)]
async fn write_stream_failure(
    repo: &Repository,
    api_key_id: &str,
    api_key_name: &str,
    session: &SessionMeta,
    mode: &str,
    model: &str,
    status_code: u16,
    error_message: &str,
    channel: Option<&Channel>,
    is_retry: bool,
    request_body: Option<&str>,
) {
    let mut log = RequestLog {
        id: utils::id::new_id(),
        seq: None,
        api_key_id: Some(api_key_id.to_string()),
        api_key_name: Some(api_key_name.to_string()),
        channel_id: channel.map(|c| c.id.clone()),
        channel_name: channel.map(|c| c.name.clone()),
        model: model.to_string(),
        upstream_model: None,
        mode: mode.to_string(),
        status_code: status_code as i64,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        duration_ms: 0,
        error_message: Some(error_message.to_string()),
        is_stream: 1,
        is_retry: if is_retry { 1 } else { 0 },
        created_at: utils::time::now_iso(),
        request_body: request_body.map(str::to_string),
        forward_body: None,
        response_choices: None,
        trace_id: None,
        response_body: None,
        session_key: None,
        client_session_id: None,
        session_source: "none".to_string(),
        session_client: None,
        session_title: None,
        request_kind: "turn".to_string(),
        history_len: None,
        risk_level: "none".to_string(),
        risk_score: 0,
        risk_summary: None,
        security_action: "none".to_string(),
        sanitized: 0,
        blocked_reason: None,
    };
    session.apply(&mut log);
    let _ = repo.create_log(&log).await;
}

// ---------- 客户端模型路由别名 ----------

/// Claude 桌面端的模型名必须是 Anthropic 风格；接入向导把路由名写入
/// `claude.desktop_aliases`，这里在入口解析成 YeAPI 的真实模型。
fn resolve_model_alias(app: &tauri::AppHandle, model: &str) -> String {
    use tauri_plugin_store::StoreExt;

    if let Ok(store) = app.store("settings.json") {
        let aliases = store.get("claude.desktop_aliases");
        if let Some(target) = aliases
            .as_ref()
            .and_then(|value| value.as_object())
            .and_then(|map| map.get(model))
            .and_then(|value| value.as_str())
        {
            return target.to_string();
        }
    }
    model.to_string()
}

// ---------- /v1/chat/completions ----------

pub async fn handle_chat_completions(
    State(shared): State<SharedState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. 解析 JSON
    let body_str = String::from_utf8_lossy(&body);
    let mut json: serde_json::Value = match serde_json::from_str(&body_str) {
        Ok(j) => j,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response(),
    };

    let is_stream = json
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let requested_model = json
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let model = resolve_model_alias(&shared.app, &requested_model);
    if model != requested_model {
        json["model"] = serde_json::json!(model);
    }

    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let anonymous_session =
        session_identity::extract(&headers, &json, Protocol::Chat, "unknown");

    // 2. API Key 鉴权
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let api_key = auth_header.strip_prefix("Bearer ").unwrap_or("").trim();

    if api_key.is_empty() {
        write_failed_log(
            &repo,
            None,
            None,
            &model,
            "chat",
            401,
            "Missing API key",
            Some(body_str.as_ref()),
            &anonymous_session,
            is_stream,
        )
        .await;
        return (StatusCode::UNAUTHORIZED, "Missing API key").into_response();
    }

    let key_record = match repo.get_api_key_by_key(api_key).await {
        Ok(k) => k,
        Err(_) => {
            write_failed_log(
                &repo,
                None,
                None,
                &model,
                "chat",
                401,
                "Invalid API key",
                Some(body_str.as_ref()),
                &anonymous_session,
                is_stream,
            )
            .await;
            return (StatusCode::UNAUTHORIZED, "Invalid API key").into_response();
        }
    };

    let session = session_identity::extract(&headers, &json, Protocol::Chat, &key_record.id);

    // 2.5 过期检查
    if let Some(ref expires_at) = key_record.expires_at {
        if key_is_expired(expires_at) {
            write_failed_log(
                &repo,
                Some(&key_record.id),
                Some(&key_record.name),
                &model,
                "chat",
                401,
                "API key has expired",
                Some(body_str.as_ref()),
                &session,
                is_stream,
            )
            .await;
            return error_response(401, "API key has expired");
        }
    }

    // 3. 配额预检：估算输入 + 输出预算（不把 max_tokens 当预计消耗）
    let quota_remaining = quota::remaining(key_record.quota_limit, key_record.quota_used);
    let requested_max_tokens = json.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let estimated_input = quota::estimate_input_tokens(&json);
    let quota_budget =
        match quota::decide_quota(quota_remaining, estimated_input, requested_max_tokens) {
            Ok(budget) => budget,
            Err(message) => {
                write_failed_log(
                    &repo,
                    Some(&key_record.id),
                    Some(&key_record.name),
                    &model,
                    "chat",
                    429,
                    &message,
                    Some(body_str.as_ref()),
                    &session,
                    is_stream,
                )
                .await;
                return error_response(429, &message);
            }
        };

    // 4. 保存原始请求体（日志用）
    let request_body_str = serde_json::to_string(&json).unwrap_or_default();

    // 4.1 把上游 max_tokens 收敛到剩余预算（原始请求体已用于日志）
    let quota_for_clamp = quota_remaining.map(|_| quota_budget);
    if let Some(effective) = quota::clamp_max_tokens(quota_for_clamp.as_ref(), requested_max_tokens)
    {
        json["max_tokens"] = serde_json::json!(effective);
    }

    // 4.5 追踪 ID（下游可注入）
    let trace_id = protocol::extract_trace_id(&headers);

    // 5. 分流：流式 vs 非流式
    if is_stream {
        handle_stream(
            shared,
            json,
            key_record.id,
            key_record.name,
            request_body_str,
            trace_id,
            session,
        )
        .await
    } else {
        match proxy::handle_request(
            &repo,
            &shared.app,
            &key_record.id,
            &key_record.name,
            json,
            false,
            Some(request_body_str),
            trace_id,
            "chat",
            &session,
        )
        .await
        {
            Ok(result) => {
                if let Ok(body) = serde_json::to_string(&result.body) {
                    let _ = repo.update_log_response_body(&result.log_id, &body).await;
                }
                (StatusCode::OK, Json(result.body)).into_response()
            }
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
                let prompt = usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let completion = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let total = usage
                    .get("total_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
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
    session: SessionMeta,
) -> Response {
    let repo = Arc::new(Repository::new(shared.state.db.pool.clone()));
    let model = json
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

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
    let security_result = security::scan_request(
        &json,
        &security_settings,
        Some(&custom_rules),
        &disabled_builtin,
    );
    if matches!(security_result.action, SecurityAction::Block) {
        let mut log = RequestLog {
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
            response_body: None,
            session_key: None,
            client_session_id: None,
            session_source: "none".to_string(),
            session_client: None,
            session_title: None,
            request_kind: "turn".to_string(),
            history_len: None,
            risk_level: security_result.risk_level.as_str().to_string(),
            risk_score: security_result.risk_score as i64,
            risk_summary: Some(security_result.summary.clone()),
            security_action: security_result.action.as_str().to_string(),
            sanitized: 0,
            blocked_reason: security_result.blocked_reason.clone(),
        };
        session.apply(&mut log);
        let log_id = log.id.clone();
        let _ = repo.create_log(&log).await;
        let _ = repo
            .create_security_findings(
                &log_id,
                &security_result.findings,
                security_result.action.as_str(),
            )
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
        let message = format!("No channel available for model: {}", model);
        write_stream_failure(
            &repo,
            &api_key_id,
            &api_key_name,
            &session,
            "chat",
            &model,
            503,
            &message,
            None,
            false,
            Some(&request_body),
        )
        .await;
        return error_response(503, &message);
    }

    // 重试配置
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
        let is_retry = attempt > 0;
        let request = ProxyRequest {
            model: model.clone(),
            body: json.clone(),
            stream: true,
        };

        match adaptor.forward_stream(&request, &config).await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body_str = resp.text().await.unwrap_or_default();
                    let error = format!(
                        "{}: {}",
                        channel.name,
                        body_str.chars().take(2000).collect::<String>()
                    );
                    last_error = Some(error.clone());
                    write_stream_failure(
                        &repo,
                        &api_key_id,
                        &api_key_name,
                        &session,
                        "chat",
                        &model,
                        status.as_u16(),
                        &error,
                        Some(&channel),
                        is_retry,
                        Some(&request_body),
                    )
                    .await;
                    continue;
                }

                let start = Instant::now();
                let mut log = RequestLog {
                    id: utils::id::new_id(),
                    seq: None,
                    api_key_id: Some(api_key_id.clone()),
                    api_key_name: Some(api_key_name.clone()),
                    channel_id: Some(channel.id.clone()),
                    channel_name: Some(channel.name.clone()),
                    model: model.clone(),
                    upstream_model: Some(model.clone()),
                    mode: "chat".to_string(),
                    status_code: 200,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    duration_ms: 0,
                    error_message: None,
                    is_stream: 1,
                    is_retry: if is_retry { 1 } else { 0 },
                    created_at: utils::time::now_iso(),
                    request_body: Some(request_body.clone()),
                    forward_body: None,
                    response_choices: None,
                    trace_id: trace_id.clone(),
                    response_body: None,
                    session_key: None,
                    client_session_id: None,
                    session_source: "none".to_string(),
                    session_client: None,
                    session_title: None,
                    request_kind: "turn".to_string(),
                    history_len: None,
                    risk_level: security_result.risk_level.as_str().to_string(),
                    risk_score: security_result.risk_score as i64,
                    risk_summary: if security_result.summary.is_empty() {
                        None
                    } else {
                        Some(security_result.summary.clone())
                    },
                    security_action: security_result.action.as_str().to_string(),
                    sanitized: if security_result.sanitized { 1 } else { 0 },
                    blocked_reason: security_result.blocked_reason.clone(),
                };
                session.apply(&mut log);

                let pending = PendingLog::new(
                    log,
                    repo.clone(),
                    security_result.findings.clone(),
                    security_result.action.as_str().to_string(),
                    start,
                );
                let guard = StreamLogGuard::new(pending);
                let upstream_stream = resp.bytes_stream();

                let passthrough_stream = async_stream::stream! {
                    tokio::pin!(upstream_stream);
                    let mut aggregator = ChatStreamAggregator::new(&model);
                    let mut had_error = false;

                    while let Some(chunk_result) = upstream_stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    if let Some((p, c, t)) = parse_usage_from_chunk(text) {
                                        guard.update_tokens(p, c, t);
                                    }
                                }
                                let done = aggregator.push(&bytes);
                                if done {
                                    // 客户端看到 [DONE] 可能立即断开：先落库再转发。
                                    let body = aggregator.final_response().map(|v| v.to_string());
                                    guard.finalize(200, None, body).await;
                                    guard.disarm();
                                    yield Ok::<_, std::io::Error>(bytes);
                                    break;
                                }
                                yield Ok::<_, std::io::Error>(bytes);
                            }
                            Err(e) => {
                                had_error = true;
                                let msg = format!("Stream connection interrupted: {e}");
                                guard.mark_error(msg.clone());
                                let body = aggregator.final_response().map(|v| v.to_string());
                                guard.finalize(502, Some(msg.clone()), body).await;
                                guard.disarm();
                                let err_chunk = format!(
                                    "data: {{\"error\":{{\"message\":\"{}\",\"type\":\"server_error\"}}}}\n\n",
                                    msg
                                );
                                yield Ok::<_, std::io::Error>(err_chunk.into_bytes().into());
                                yield Ok::<_, std::io::Error>(Bytes::from_static(b"data: [DONE]\n\n"));
                                break;
                            }
                        }
                    }

                    if !had_error {
                        // 上游自然结束但没看到 [DONE]：补一次落库（槽位为空时是 no-op）。
                        let body = aggregator.final_response().map(|v| v.to_string());
                        guard.finalize(200, None, body).await;
                        guard.disarm();
                    }
                };

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header(header::CONNECTION, "keep-alive")
                    .body(Body::from_stream(passthrough_stream))
                    .unwrap();
            }
            Err(e) => {
                let error = format!("{}: {}", channel.name, e);
                last_error = Some(error.clone());
                write_stream_failure(
                    &repo,
                    &api_key_id,
                    &api_key_name,
                    &session,
                    "chat",
                    &model,
                    502,
                    &error,
                    Some(&channel),
                    is_retry,
                    Some(&request_body),
                )
                .await;
            }
        }
    }

    error_response(
        502,
        &format!("All stream channels failed: {:?}", last_error),
    )
}
// ---------- 多协议流式转换（Anthropic / Responses 共用骨架） ----------

trait StreamConverter {
    fn new(model: &str) -> Self;
    fn push(&mut self, bytes: &[u8]) -> Vec<String>;
    fn finish(&mut self) -> Vec<String>;
    /// 上游流中断时的收尾事件；默认等价于正常结束（Anthropic 等协议无需改动）。
    fn finish_error(&mut self, _message: &str) -> Vec<String> {
        self.finish()
    }
    /// 正常结束后导出最终响应对象，供日志落库（默认无）。
    fn final_response(&mut self) -> Option<serde_json::Value> {
        None
    }
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
    fn final_response(&mut self) -> Option<serde_json::Value> {
        AnthropicStreamConverter::final_response(self)
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
    fn finish_error(&mut self, message: &str) -> Vec<String> {
        ResponsesStreamConverter::finish_error(self, message)
    }
    fn final_response(&mut self) -> Option<serde_json::Value> {
        ResponsesStreamConverter::final_response(self)
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
    let session = auth.session.clone();

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
    let security_result = security::scan_request(
        &openai_body,
        &security_settings,
        Some(&custom_rules),
        &disabled_builtin,
    );
    if matches!(security_result.action, SecurityAction::Block) {
        let mut log = RequestLog {
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
            response_body: None,
            session_key: None,
            client_session_id: None,
            session_source: "none".to_string(),
            session_client: None,
            session_title: None,
            request_kind: "turn".to_string(),
            history_len: None,
            risk_level: security_result.risk_level.as_str().to_string(),
            risk_score: security_result.risk_score as i64,
            risk_summary: Some(security_result.summary.clone()),
            security_action: security_result.action.as_str().to_string(),
            sanitized: 0,
            blocked_reason: security_result.blocked_reason.clone(),
        };
        session.apply(&mut log);
        let log_id = log.id.clone();
        let _ = repo.create_log(&log).await;
        let _ = repo
            .create_security_findings(
                &log_id,
                &security_result.findings,
                security_result.action.as_str(),
            )
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
        let message = format!("No channel available for model: {}", model);
        write_stream_failure(
            &repo,
            &auth.api_key_id,
            &auth.api_key_name,
            &session,
            mode,
            &model,
            503,
            &message,
            None,
            false,
            Some(&request_body),
        )
        .await;
        return error_response(503, &message);
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
        let is_retry = attempt > 0;
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
                    let error = format!(
                        "{}: {}",
                        channel.name,
                        body_str.chars().take(2000).collect::<String>()
                    );
                    last_error = Some(error.clone());
                    write_stream_failure(
                        &repo,
                        &auth.api_key_id,
                        &auth.api_key_name,
                        &session,
                        mode,
                        &model,
                        status.as_u16(),
                        &error,
                        Some(&channel),
                        is_retry,
                        Some(&request_body),
                    )
                    .await;
                    continue;
                }

                let start = Instant::now();
                let model_c = model.clone();
                let mut log = RequestLog {
                    id: utils::id::new_id(),
                    seq: None,
                    api_key_id: Some(auth.api_key_id.clone()),
                    api_key_name: Some(auth.api_key_name.clone()),
                    channel_id: Some(channel.id.clone()),
                    channel_name: Some(channel.name.clone()),
                    model: model.clone(),
                    upstream_model: Some(model.clone()),
                    mode: mode.to_string(),
                    status_code: 200,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    duration_ms: 0,
                    error_message: None,
                    is_stream: 1,
                    is_retry: if is_retry { 1 } else { 0 },
                    created_at: utils::time::now_iso(),
                    request_body: Some(request_body.clone()),
                    forward_body: None,
                    response_choices: None,
                    trace_id: trace_id.clone(),
                    response_body: None,
                    session_key: None,
                    client_session_id: None,
                    session_source: "none".to_string(),
                    session_client: None,
                    session_title: None,
                    request_kind: "turn".to_string(),
                    history_len: None,
                    risk_level: security_result.risk_level.as_str().to_string(),
                    risk_score: security_result.risk_score as i64,
                    risk_summary: if security_result.summary.is_empty() {
                        None
                    } else {
                        Some(security_result.summary.clone())
                    },
                    security_action: security_result.action.as_str().to_string(),
                    sanitized: if security_result.sanitized { 1 } else { 0 },
                    blocked_reason: security_result.blocked_reason.clone(),
                };
                session.apply(&mut log);

                let pending = PendingLog::new(
                    log,
                    repo.clone(),
                    security_result.findings.clone(),
                    security_result.action.as_str().to_string(),
                    start,
                );
                let guard = StreamLogGuard::new(pending);
                let upstream_stream = resp.bytes_stream();

                let converted_stream = async_stream::stream! {
                    tokio::pin!(upstream_stream);

                    // 先给客户端一个 SSE 注释，之后每 10 秒一次。
                    yield Ok::<_, std::io::Error>(Bytes::from(": keep-alive\n\n"));
                    let mut keepalive =
                        tokio::time::interval(std::time::Duration::from_secs(10));
                    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    keepalive.tick().await;

                    let mut converter = C::new(&model_c);
                    let mut usage_prompt: i64 = 0;
                    let mut usage_completion: i64 = 0;
                    let mut usage_total: i64 = 0;
                    let mut had_error = false;
                    let mut stream_error: Option<String> = None;

                    loop {
                        tokio::select! {
                            _ = keepalive.tick() => {
                                yield Ok::<_, std::io::Error>(Bytes::from(": keep-alive\n\n"));
                            }
                            chunk_result = upstream_stream.next() => {
                                match chunk_result {
                                    Some(Ok(bytes)) => {
                                        if let Ok(text) = std::str::from_utf8(&bytes) {
                                            if let Some((p, c, t)) = protocol::parse_usage_from_sse_chunk(text) {
                                                usage_prompt = p;
                                                usage_completion = c;
                                                usage_total = t;
                                                guard.update_tokens(p, c, t);
                                            }
                                        }
                                        let mut terminal_events = Vec::new();
                                        let mut has_terminal = false;
                                        for ev in converter.push(&bytes) {
                                            if ev.starts_with("event: response.completed")
                                                || ev.starts_with("event: response.failed")
                                                || ev.starts_with("event: message_stop")
                                            {
                                                has_terminal = true;
                                                terminal_events.push(ev);
                                            } else {
                                                yield Ok::<_, std::io::Error>(Bytes::from(ev));
                                            }
                                        }
                                        if has_terminal {
                                            // 终止事件一旦发给客户端，Codex 可能立即断开；
                                            // 必须先落库与扣配额，再发送终止事件。
                                            let body = converter
                                                .final_response()
                                                .map(|value| value.to_string());
                                            guard.finalize(200, None, body).await;
                                            guard.disarm();
                                            for ev in terminal_events {
                                                yield Ok::<_, std::io::Error>(Bytes::from(ev));
                                            }
                                            return;
                                        }
                                    }
                                    Some(Err(e)) => {
                                        had_error = true;
                                        let msg = format!("Upstream stream interrupted: {e}");
                                        guard.mark_error(msg.clone());
                                        stream_error = Some(msg);
                                        break;
                                    }
                                    None => break,
                                }
                            }
                        }
                    }

                    let tail = if had_error {
                        converter.finish_error(
                            stream_error.as_deref().unwrap_or("upstream stream interrupted"),
                        )
                    } else {
                        converter.finish()
                    };
                    let response_body = converter.final_response().map(|v| v.to_string());

                    // 关键：先落库 + 扣配额，再发送终止事件。
                    guard
                        .finalize(
                            if had_error { 502 } else { 200 },
                            stream_error,
                            response_body,
                        )
                        .await;
                    guard.disarm();

                    for ev in tail {
                        yield Ok::<_, std::io::Error>(Bytes::from(ev));
                    }
                };

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header(header::CONNECTION, "keep-alive")
                    .header("x-accel-buffering", "no")
                    .body(Body::from_stream(converted_stream))
                    .unwrap();
            }
            Err(e) => {
                let error = format!("{}: {}", channel.name, e);
                last_error = Some(error.clone());
                write_stream_failure(
                    &repo,
                    &auth.api_key_id,
                    &auth.api_key_name,
                    &session,
                    mode,
                    &model,
                    502,
                    &error,
                    Some(&channel),
                    is_retry,
                    Some(&request_body),
                )
                .await;
            }
        }
    }

    error_response(
        502,
        &format!("All stream channels failed: {:?}", last_error),
    )
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
    let mut openai_body = protocol::anthropic_to_openai(&json);
    let requested_model = openai_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let model = resolve_model_alias(&shared.app, &requested_model);
    if model != requested_model {
        openai_body["model"] = serde_json::json!(model);
    }
    let is_stream = openai_body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let max_tokens = openai_body
        .get("max_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let request_body_str = serde_json::to_string(&json).unwrap_or_default();

    // 鉴权 + 配额
    let auth = match authenticate_and_check_quota(
        &shared,
        &headers,
        &model,
        &openai_body,
        Protocol::Messages,
        "messages",
        &json,
        Some(&request_body_str),
        max_tokens,
    )
    .await
    {
        Ok(a) => a,
        Err((code, msg)) => return anthropic_error_response(code, &msg),
    };

    if let Some(effective) = quota::clamp_max_tokens(auth.quota.as_ref(), max_tokens) {
        openai_body["max_tokens"] = serde_json::json!(effective);
    }

    let trace_id = protocol::extract_trace_id(&headers);

    if is_stream {
        handle_messages_stream(
            shared,
            auth,
            openai_body,
            model,
            request_body_str,
            trace_id,
        )
        .await
    } else {
        let session = auth.session.clone();
        match proxy::handle_request(
            &auth.repo,
            &shared.app,
            &auth.api_key_id,
            &auth.api_key_name,
            openai_body,
            false,
            Some(request_body_str),
            trace_id,
            "messages",
            &session,
        )
        .await
        {
            Ok(result) => {
                let anthropic_resp = protocol::openai_to_anthropic(&result.body, &model);
                if let Ok(body) = serde_json::to_string(&anthropic_resp) {
                    let _ = auth
                        .repo
                        .update_log_response_body(&result.log_id, &body)
                        .await;
                }
                (
                    StatusCode::from_u16(result.status).unwrap_or(StatusCode::OK),
                    Json(anthropic_resp),
                )
                    .into_response()
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
    proxy_stream::<AnthropicStreamConverter>(
        shared,
        auth,
        openai_body,
        model,
        request_body,
        trace_id,
        "messages",
    )
    .await
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
    let mut openai_body = protocol::responses_to_openai(&json);
    let requested_model = openai_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let model = resolve_model_alias(&shared.app, &requested_model);
    if model != requested_model {
        openai_body["model"] = serde_json::json!(model);
    }
    let is_stream = openai_body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let max_tokens = openai_body
        .get("max_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let request_body_str = serde_json::to_string(&json).unwrap_or_default();

    // 鉴权 + 配额
    let auth = match authenticate_and_check_quota(
        &shared,
        &headers,
        &model,
        &openai_body,
        Protocol::Responses,
        "responses",
        &json,
        Some(&request_body_str),
        max_tokens,
    )
    .await
    {
        Ok(a) => a,
        Err((code, msg)) => return error_response(code, &msg),
    };

    if let Some(effective) = quota::clamp_max_tokens(auth.quota.as_ref(), max_tokens) {
        openai_body["max_tokens"] = serde_json::json!(effective);
    }

    let trace_id = protocol::extract_trace_id(&headers);

    if is_stream {
        handle_responses_stream(shared, auth, openai_body, model, request_body_str, trace_id).await
    } else {
        let session = auth.session.clone();
        match proxy::handle_request(
            &auth.repo,
            &shared.app,
            &auth.api_key_id,
            &auth.api_key_name,
            openai_body,
            false,
            Some(request_body_str),
            trace_id,
            "responses",
            &session,
        )
        .await
        {
            Ok(result) => {
                let responses_resp = protocol::openai_to_responses(&result.body, &model);
                if let Ok(body) = serde_json::to_string(&responses_resp) {
                    let _ = auth
                        .repo
                        .update_log_response_body(&result.log_id, &body)
                        .await;
                }
                (
                    StatusCode::from_u16(result.status).unwrap_or(StatusCode::OK),
                    Json(responses_resp),
                )
                    .into_response()
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
    proxy_stream::<ResponsesStreamConverter>(
        shared,
        auth,
        openai_body,
        model,
        request_body,
        trace_id,
        "responses",
    )
    .await
}

// ---------- /v1/models ----------

pub async fn handle_list_models(State(shared): State<SharedState>) -> Response {
    let repo = Repository::new(shared.state.db.pool.clone());
    match repo.get_enabled_channels().await {
        Ok(channels) => {
            let created = chrono::Utc::now().timestamp();
            let models: Vec<serde_json::Value> = collect_models(&channels)
                .into_iter()
                .map(|(id, owned_by)| {
                    serde_json::json!({
                        "id": id,
                        "object": "model",
                        "created": created,
                        "owned_by": owned_by,
                    })
                })
                .collect();
            Json(serde_json::json!({ "object": "list", "data": models })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
            .into_response(),
    }
}

// ---------- /health ----------

pub async fn handle_health(State(shared): State<SharedState>) -> Response {
    let port = shared.state.server_port.read().await.clone();
    let running = shared
        .state
        .server_running
        .load(std::sync::atomic::Ordering::SeqCst);
    Json(serde_json::json!({
        "status": "ok",
        "running": running,
        "port": port,
        "url": format!("http://127.0.0.1:{}", port),
    }))
    .into_response()
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
