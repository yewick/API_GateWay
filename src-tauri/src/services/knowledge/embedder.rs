//! Embeddings 向量化：复用渠道调度能力调用 OpenAI 兼容 Embeddings API。
//!
//! 与聊天 adaptor 不同，本模块直接 POST `{base_url}/embeddings`（adaptor 硬编码
//! `/chat/completions`，不可复用），仅复用「选渠道 + 取 base_url/api_key」。

use crate::adaptor::openai::apply_model_mapping;
use crate::core::dispatcher::Dispatcher;
use crate::db::models::{Channel, RequestLog};
use crate::db::repository::Repository;
use crate::utils;

/// Embedding 调用产生的 Token 用量（OpenAI 兼容 `usage` 无 completion）。
pub struct EmbeddingUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

/// 校验知识库创建所需的 embedding 配置：模型与渠道均非空，且所选渠道启用并支持该模型。
/// 与 [`embed`] 的渠道调度同源（`Dispatcher::select_channels`），不满足返回中文错误。
pub async fn validate_embedding_config(
    repo: &Repository,
    embedding_model: Option<&str>,
    embedding_channel_id: Option<&str>,
) -> Result<(), String> {
    let model = embedding_model
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or("请选择向量模型（embedding_model）")?;
    let channel_id = embedding_channel_id
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or("请选择向量渠道（embedding_channel_id）")?;

    let enabled = repo
        .get_enabled_channels()
        .await
        .map_err(|e| format!("读取启用渠道失败: {e}"))?;

    let channel = enabled
        .iter()
        .find(|c| c.id == channel_id)
        .ok_or("所选向量渠道不存在或未启用，请前往「渠道」配置")?;

    let selected = Dispatcher::select_channels(&enabled, model);
    if !selected.iter().any(|c| c.id == channel_id) {
        return Err(format!(
            "所选向量渠道「{}」不支持模型「{}」，请前往「渠道」配置",
            channel.name, model
        ));
    }

    Ok(())
}

/// 向量化一批文本。按渠道优先级逐个尝试，任一成功即返回；全败返回错误。
/// `attribution` 为 `Some((api_key_id, api_key_name))` 时，成功后写一条 `mode="embedding"` 日志并扣配额。
pub async fn embed(
    texts: &[String],
    model: &str,
    channel_id: Option<&str>,
    repo: &Repository,
    attribution: Option<(&str, &str)>,
) -> Result<Vec<Vec<f32>>, String> {
    let enabled = repo
        .get_enabled_channels()
        .await
        .map_err(|e| format!("读取启用渠道失败: {e}"))?;

    // 1. 指定渠道优先（若命中启用渠道）
    let mut ordered: Vec<Channel> = Vec::new();
    if let Some(cid) = channel_id {
        if let Some(c) = enabled.iter().find(|c| c.id == cid) {
            ordered.push(c.clone());
        }
    }

    // 2. 按模型调度补齐（select_channels 内部过滤 status=1 + 模型匹配 + 优先级/权重）
    let selected = Dispatcher::select_channels(&enabled, model);
    for c in selected {
        if !ordered.iter().any(|o| o.id == c.id) {
            ordered.push(c);
        }
    }

    // 3. 兜底：模型匹配为空时回退到全部启用渠道
    if ordered.is_empty() {
        ordered = enabled.clone();
    }

    if ordered.is_empty() {
        return Err(format!("没有可用的 Embedding 渠道（模型 {model}）"));
    }

    let mut last_err = String::new();
    for channel in &ordered {
        match try_embed_with_channel(texts, model, channel).await {
            Ok((vecs, usage, upstream_model)) => {
                if let Some((key_id, key_name)) = attribution {
                    let total = usage.as_ref().map(|u| u.total_tokens as i64).unwrap_or(0);
                    let log = RequestLog {
                        id: utils::id::new_id(),
                        seq: None,
                        api_key_id: Some(key_id.to_string()),
                        api_key_name: Some(key_name.to_string()),
                        channel_id: Some(channel.id.clone()),
                        channel_name: Some(channel.name.clone()),
                        model: model.to_string(),
                        upstream_model: Some(upstream_model),
                        mode: "embedding".to_string(),
                        status_code: 200,
                        prompt_tokens: usage.as_ref().map(|u| u.prompt_tokens as i64).unwrap_or(0),
                        completion_tokens: 0,
                        total_tokens: total,
                        duration_ms: 0,
                        error_message: None,
                        is_stream: 0,
                        is_retry: 0,
                        created_at: utils::time::now_iso(),
                        request_body: None,
                        forward_body: None,
                        response_choices: None,
                        trace_id: None,
                        risk_level: "none".to_string(),
                        risk_score: 0,
                        risk_summary: None,
                        security_action: "none".to_string(),
                        sanitized: 0,
                        blocked_reason: None,
                    };
                    let _ = repo.create_log(&log).await;
                    if total > 0 {
                        let _ = repo.increment_quota(key_id, total).await;
                    }
                }
                return Ok(vecs);
            }
            Err(e) => last_err = e,
        }
    }
    Err(format!("所有渠道均失败（模型 {model}）：{last_err}"))
}

/// 对单个渠道发起 `/embeddings` 请求并解析结果。
/// 返回 `(向量列表, Token 用量, 上游真实模型名)`。
async fn try_embed_with_channel(
    texts: &[String],
    model: &str,
    channel: &Channel,
) -> Result<(Vec<Vec<f32>>, Option<EmbeddingUsage>, String), String> {
    let base = channel.base_url.trim_end_matches('/');
    let url = format!("{base}/embeddings");
    let body = serde_json::json!({ "model": model, "input": texts });

    // 与聊天链路一致：应用渠道 model_mapping，把「面向下游的模型名」翻译为「上游真实模型名」
    let mapping: serde_json::Value =
        serde_json::from_str(&channel.model_mapping).unwrap_or(serde_json::Value::Null);
    let body = apply_model_mapping(&body, &mapping);
    let upstream_model = mapping
        .get(model)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| model.to_string());

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", channel.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("渠道 {} 请求失败: {e}", channel.name))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("渠道 {} 读取响应失败: {e}", channel.name))?;

    if !status.is_success() {
        return Err(format!("渠道 {} 返回 {status}: {}", channel.name, text));
    }

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("渠道 {} 响应非 JSON: {e}", channel.name))?;
    let vecs = parse_embeddings_response(&json).map_err(|e| format!("渠道 {}: {e}", channel.name))?;
    let usage = json.get("usage").map(|u| EmbeddingUsage {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
    });
    Ok((vecs, usage, upstream_model))
}

/// 纯函数：解析 Embeddings 响应 `data[].embedding`，校验所有向量维度一致。
pub fn parse_embeddings_response(body: &serde_json::Value) -> Result<Vec<Vec<f32>>, String> {
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or("响应缺少 data 数组")?;

    let mut out: Vec<Vec<f32>> = Vec::with_capacity(data.len());
    for (i, item) in data.iter().enumerate() {
        let emb = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| format!("第 {i} 项缺少 embedding"))?;
        let vec: Vec<f32> = emb
            .iter()
            .map(|x| {
                x.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| format!("第 {i} 项 embedding 含非数值"))
            })
            .collect::<Result<_, _>>()?;
        out.push(vec);
    }

    if out.is_empty() {
        return Err("响应 data 为空".to_string());
    }
    let dim = out[0].len();
    if dim == 0 {
        return Err("embedding 维度为 0".to_string());
    }
    for (i, v) in out.iter().enumerate() {
        if v.len() != dim {
            return Err(format!(
                "embedding 维度不一致：第 0 项 {dim} vs 第 {i} 项 {}",
                v.len()
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_embeddings_response_ok() {
        let body = serde_json::json!({
            "data": [
                {"embedding": [0.1, 0.2, 0.3]},
                {"embedding": [0.4, 0.5, 0.6]}
            ]
        });
        let v = parse_embeddings_response(&body).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], vec![0.1f32, 0.2, 0.3]);
        assert_eq!(v[1], vec![0.4f32, 0.5, 0.6]);
    }

    #[test]
    fn test_parse_embeddings_response_dim_mismatch() {
        let body = serde_json::json!({
            "data": [
                {"embedding": [0.1, 0.2]},
                {"embedding": [0.1, 0.2, 0.3]}
            ]
        });
        assert!(parse_embeddings_response(&body).is_err());
    }

    #[test]
    fn test_parse_embeddings_response_missing_data() {
        let body = serde_json::json!({"error": "x"});
        assert!(parse_embeddings_response(&body).is_err());
    }

    #[test]
    fn test_parse_embeddings_response_empty() {
        let body = serde_json::json!({"data": []});
        assert!(parse_embeddings_response(&body).is_err());
    }
}
