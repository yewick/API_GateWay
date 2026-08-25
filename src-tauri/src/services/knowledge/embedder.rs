//! Embeddings 向量化：复用渠道调度能力调用 OpenAI 兼容 Embeddings API。
//!
//! 与聊天 adaptor 不同，本模块直接 POST `{base_url}/embeddings`（adaptor 硬编码
//! `/chat/completions`，不可复用），仅复用「选渠道 + 取 base_url/api_key」。

use crate::core::dispatcher::Dispatcher;
use crate::db::models::Channel;
use crate::db::repository::Repository;

/// 向量化一批文本。按渠道优先级逐个尝试，任一成功即返回；全败返回错误。
pub async fn embed(
    texts: &[String],
    model: &str,
    channel_id: Option<&str>,
    repo: &Repository,
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
            Ok(v) => return Ok(v),
            Err(e) => last_err = e,
        }
    }
    Err(format!("所有渠道均失败（模型 {model}）：{last_err}"))
}

/// 对单个渠道发起 `/embeddings` 请求并解析结果。
async fn try_embed_with_channel(
    texts: &[String],
    model: &str,
    channel: &Channel,
) -> Result<Vec<Vec<f32>>, String> {
    let base = channel.base_url.trim_end_matches('/');
    let url = format!("{base}/embeddings");
    let body = serde_json::json!({ "model": model, "input": texts });

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
    parse_embeddings_response(&json).map_err(|e| format!("渠道 {}: {e}", channel.name))
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
