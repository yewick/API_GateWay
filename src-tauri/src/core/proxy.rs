use crate::adaptor::{get_adaptor, ProxyRequest, TokenUsage};
use crate::core::dispatcher::Dispatcher;
use crate::db::models::{Channel, RequestLog};
use crate::db::repository::Repository;
use crate::security::{self, SecurityAction};
use crate::utils;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub struct ProxyResult {
    pub status: u16,
    pub body: serde_json::Value,
    pub usage: Option<TokenUsage>,
    pub channel: Channel,
    pub duration_ms: u64,
}

/// 核心转发：鉴权、安全扫描、渠道调度、故障转移、日志与配额
pub async fn handle_request(
    repo: &Arc<Repository>,
    app: &AppHandle,
    api_key_id: &str,
    api_key_name: &str,
    body: serde_json::Value,
    is_stream: bool,
    request_body: Option<String>,
    trace_id: Option<String>,
) -> Result<ProxyResult, (u16, String)> {
    let start = Instant::now();
    let model = body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // 读取安全配置并扫描请求体
    let security_settings = security::get_security_settings(app);
    // 加载自定义规则（黑名单/白名单）
    let custom_rules = repo
        .get_enabled_custom_rules()
        .await
        .unwrap_or_default();
    // 加载被禁用的内置规则（enabled=0 的 rule_id 集合）
    let disabled_builtin: HashSet<String> = repo
        .get_all_builtin_rules()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.enabled == 0)
        .map(|r| r.rule_id)
        .collect();
    let mut security_result =
        security::scan_request(&body, &security_settings, Some(&custom_rules), &disabled_builtin);

    // 需要脱敏时在转发前改写请求体
    let (forward_body, was_redacted) = if matches!(security_result.action, SecurityAction::Redact)
        || security_settings.redact_secrets
    {
        let (b, changed) = security::redact_request_body(&body, &security_settings);
        (b, changed)
    } else {
        (body.clone(), false)
    };
    if was_redacted {
        security_result.sanitized = true;
    }

    // 策略判定为阻断：记录日志后返回 451
    if matches!(security_result.action, SecurityAction::Block) {
        let log = RequestLog {
            id: utils::id::new_id(),
            seq: None,
            api_key_id: Some(api_key_id.to_string()),
            api_key_name: Some(api_key_name.to_string()),
            channel_id: None,
            channel_name: None,
            model: model.clone(),
            upstream_model: None,
            mode: "chat".to_string(),
            status_code: 451,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            duration_ms: start.elapsed().as_millis() as i64,
            error_message: security_result.blocked_reason.clone(),
            is_stream: if is_stream { 1 } else { 0 },
            is_retry: 0,
            created_at: utils::time::now_iso(),
            request_body: request_body.clone(),
            forward_body: None,
            response_choices: None,
            trace_id: trace_id.clone(),
            risk_level: security_result.risk_level.as_str().to_string(),
            risk_score: security_result.risk_score as i64,
            risk_summary: Some(security_result.summary.clone()),
            security_action: security_result.action.as_str().to_string(),
            sanitized: if security_result.sanitized { 1 } else { 0 },
            blocked_reason: security_result.blocked_reason.clone(),
        };
        let log_id = log.id.clone();
        let _ = repo.create_log(&log).await;
        let _ = repo
            .create_security_findings(&log_id, &security_result.findings, security_result.action.as_str())
            .await;
        return Err((451, security_result.summary));
    }

    // 获取启用渠道并按模型调度
    let channels = repo
        .get_enabled_channels()
        .await
        .map_err(|e| (500, format!("DB error: {}", e)))?;
    if channels.is_empty() {
        return Err((503, "No available channels".to_string()));
    }
    let selected_channels = Dispatcher::select_channels(&channels, &model);
    if selected_channels.is_empty() {
        return Err((503, format!("No channel available for model: {}", model)));
    }

    let request = ProxyRequest {
        model: model.clone(),
        body: forward_body.clone(),
        stream: is_stream,
    };

    // 重试配置
    let (retry_enabled, retry_times) = get_retry_settings(app);
    let max_attempts = if retry_enabled {
        (retry_times.max(0) as usize + 1).min(selected_channels.len())
    } else {
        1
    };

    let mut last_error: Option<String> = None;

    for (attempt, channel) in selected_channels.into_iter().take(max_attempts).enumerate() {
        let config = Dispatcher::channel_to_config(&channel);
        let adaptor = get_adaptor(&channel.channel_type);
        let attempt_start = Instant::now();
        let result = adaptor.forward(&request, &config).await;
        let duration_ms = attempt_start.elapsed().as_millis() as u64;
        let is_retry = if attempt > 0 { 1 } else { 0 };

        // 计算映射后的上游真实模型名
        let upstream_model = config
            .model_mapping
            .get(model.as_str())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| model.clone());

        match result {
            Ok((status, resp_body, usage)) => {
                // 响应侧安全扫描（可选）
                if !security_result.findings.is_empty() || security_settings.scan_response {
                    let resp_security = security::scan_response(&resp_body, &security_settings, Some(&custom_rules), &disabled_builtin);
                    if !resp_security.findings.is_empty() {
                        security_result.findings.extend(resp_security.findings);
                        if resp_security.risk_level.rank() > security_result.risk_level.rank() {
                            security_result.risk_level = resp_security.risk_level;
                            security_result.risk_score = security_result.risk_score.max(resp_security.risk_score);
                            security_result.summary = format!(
                                "{} | 响应侧: {}",
                                security_result.summary, resp_security.summary
                            );
                        }
                    }
                }

                let log = RequestLog {
                    id: utils::id::new_id(),
                    seq: None,
                    api_key_id: Some(api_key_id.to_string()),
                    api_key_name: Some(api_key_name.to_string()),
                    channel_id: Some(channel.id.clone()),
                    channel_name: Some(channel.name.clone()),
                    model: model.clone(),
                    upstream_model: Some(upstream_model.clone()),
                    mode: "chat".to_string(),
                    status_code: status as i64,
                    prompt_tokens: usage.as_ref().map(|u| u.prompt_tokens as i64).unwrap_or(0),
                    completion_tokens: usage.as_ref().map(|u| u.completion_tokens as i64).unwrap_or(0),
                    total_tokens: usage.as_ref().map(|u| u.total_tokens as i64).unwrap_or(0),
                    duration_ms: duration_ms as i64,
                    error_message: None,
                    is_stream: if is_stream { 1 } else { 0 },
                    is_retry,
                    created_at: utils::time::now_iso(),
                    request_body: request_body.clone(),
                    forward_body: if was_redacted {
                        serde_json::to_string(&forward_body).ok()
                    } else {
                        None
                    },
                    response_choices: resp_body.get("choices").map(|c| c.to_string()),
                    trace_id: trace_id.clone(),
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
                let log_id = log.id.clone();
                let _ = repo.create_log(&log).await;
                let _ = repo
                    .create_security_findings(&log_id, &security_result.findings, security_result.action.as_str())
                    .await;

                // 配额扣减
                if let Some(ref u) = usage {
                    let _ = repo.increment_quota(api_key_id, u.total_tokens as i64).await;
                }

                return Ok(ProxyResult {
                    status,
                    body: resp_body,
                    usage,
                    channel,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                let error_message = e.to_string();
                let log = RequestLog {
                    id: utils::id::new_id(),
                    seq: None,
                    api_key_id: Some(api_key_id.to_string()),
                    api_key_name: Some(api_key_name.to_string()),
                    channel_id: Some(channel.id.clone()),
                    channel_name: Some(channel.name.clone()),
                    model: model.clone(),
                    upstream_model: Some(upstream_model.clone()),
                    mode: "chat".to_string(),
                    status_code: 502,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    duration_ms: duration_ms as i64,
                    error_message: Some(error_message.clone()),
                    is_stream: if is_stream { 1 } else { 0 },
                    is_retry,
                    created_at: utils::time::now_iso(),
                    request_body: request_body.clone(),
                    forward_body: if was_redacted {
                        serde_json::to_string(&forward_body).ok()
                    } else {
                        None
                    },
                    response_choices: None,
                    trace_id: trace_id.clone(),
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
                let log_id = log.id.clone();
                let _ = repo.create_log(&log).await;
                let _ = repo
                    .create_security_findings(&log_id, &security_result.findings, security_result.action.as_str())
                    .await;
                last_error = Some(error_message);
            }
        }
    }

    Err((
        502,
        format!(
            "All channels failed for model {} after {} attempt(s): {}",
            model,
            max_attempts,
            last_error.unwrap_or_else(|| "unknown upstream error".to_string())
        ),
    ))
}

/// 读取重试配置（Tauri Store）
pub fn get_retry_settings(app: &AppHandle) -> (bool, i32) {
    if let Ok(store) = app.store("settings.json") {
        let enabled = store
            .get("retry.enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let times = store
            .get("retry.times")
            .and_then(|v| v.as_i64())
            .unwrap_or(2) as i32;
        return (enabled, times);
    }
    (true, 2) // 默认启用重试，最多 2 次
}
