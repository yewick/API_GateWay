//! RAG 问答层：检索 → 上下文装配 → 上下文上限解析与降级 → 密钥分发（代理）→ 生成。
//!
//! 向量化（[`super::embedder`]）仍按「启用渠道 → 模型匹配」直接分派；LLM 生成则经
//! [`crate::core::proxy::handle_request`] 走密钥分发，与网关普通请求一致（安全扫描、渠道调度、
//! 重试、配额扣减、日志），因此需要 `AppHandle` 读取安全/重试配置。

use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::adaptor::TokenUsage;
use crate::core::dispatcher::Dispatcher;
use crate::core::proxy;
use crate::db::models::{ApiKey, Channel};
use crate::db::repository::Repository;

use super::embedder;
use super::models::*;
use super::repository::KbRepository;
use super::retriever;
use super::splitter;

/// 默认 Embedding 模型（全局搜索 / 知识库未配置时）
pub const DEFAULT_EMBEDDING_MODEL: &str = "embedding-3";
/// 默认上下文上限（token 数，兜底）
const DEFAULT_CONTEXT_LIMIT: u64 = 32768;
/// 上下文上限环境变量名
const CONTEXT_LIMIT_ENV: &str = "YEAPI_KB_CONTEXT_LIMIT";
/// 最近历史条数（无显式 history 时从 DB 取）
const RECENT_HISTORY_ROUNDS: usize = 6;

/// 全局（未指定 KB）检索/问答的默认 Embedding 模型：
/// 优先读 store 的 `knowledge.default_embedding_model`，未配置回退 [`DEFAULT_EMBEDDING_MODEL`]。
/// 不用环境变量——默认值须前端可调（见 todo §8.1）。
pub fn default_embedding_model(app: &AppHandle) -> String {
    use tauri_plugin_store::StoreExt;
    if let Ok(store) = app.store("settings.json") {
        if let Some(model) = store
            .get("knowledge.default_embedding_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            let m = model.trim();
            if !m.is_empty() {
                return m.to_string();
            }
        }
    }
    DEFAULT_EMBEDDING_MODEL.to_string()
}

/// RAG 问答主流程：向量化 query → 检索 → 历史 → 上下文装配 → LLM 生成 → 持久化。
pub async fn ask(
    pool: &SqlitePool,
    app: &AppHandle,
    kb_id: &str,
    query: &str,
    chat_model: &str,
    top_k: usize,
    mcp_only: bool,
    history: Option<&[ConversationMessage]>,
    context_limit_override: Option<u64>,
    api_key: Option<ApiKey>,
) -> Result<RagAnswer, String> {
    let repo = KbRepository::new(pool.clone());
    let db = Arc::new(Repository::new(pool.clone()));

    // 0. 确定归属密钥：显式传入（HTTP 鉴权）优先，否则内部选一个启用密钥（Tauri 命令路径）
    let api_key = match api_key {
        Some(k) => k,
        None => select_api_key(&db, chat_model).await?,
    };

    // 1. 确定 embedding 模型与指定渠道
    let (embedding_model, embedding_channel_id) = if kb_id.is_empty() {
        (default_embedding_model(app), None)
    } else {
        let kb = repo
            .get_kb(kb_id)
            .await
            .map_err(|e| format!("读取知识库失败: {e}"))?;
        (
            kb.embedding_model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| default_embedding_model(app)),
            kb.embedding_channel_id.clone(),
        )
    };

    // 2. 向量化 query（归属到选定密钥，计入日志/配额）
    let vecs =
        embedder::embed(&[query.to_string()], &embedding_model, embedding_channel_id.as_deref(), db.as_ref(), Some((api_key.id.as_str(), api_key.name.as_str())))
            .await?;
    let query_emb = vecs.into_iter().next().ok_or("向量化返回空结果")?;

    // 3. 检索
    let results = if kb_id.is_empty() {
        retriever::search_all(&repo, query, &query_emb, top_k, mcp_only).await?
    } else {
        retriever::hybrid_search(
            &repo,
            kb_id,
            query,
            &query_emb,
            top_k,
            retriever::VECTOR_WEIGHT,
            retriever::KEYWORD_WEIGHT,
        )
        .await?
    };

    if results.is_empty() {
        return Ok(RagAnswer {
            answer: "知识库中没有找到相关内容。".to_string(),
            sources: Vec::new(),
            usage: None,
        });
    }

    // 4. 历史：优先显式传入，否则回退 DB 最近若干条（仅指定 KB 时）
    let history_vec: Vec<ConversationMessage> = match history {
        Some(h) if !h.is_empty() => h.to_vec(),
        _ if kb_id.is_empty() => Vec::new(),
        _ => {
            let convs = repo
                .get_conversations(kb_id)
                .await
                .map_err(|e| format!("读取对话历史失败: {e}"))?;
            convs
                .into_iter()
                .rev()
                .take(RECENT_HISTORY_ROUNDS)
                .rev()
                .map(|c| ConversationMessage {
                    role: c.role,
                    content: c.content,
                })
                .collect()
        }
    };

    // 5. LLM 生成（解析上下文上限、经代理转发，密钥由上层指定）
    let (body, usage) =
        chat_completion(&db, app, &api_key, chat_model, &results, query, &history_vec, context_limit_override)
            .await?;
    let answer = extract_answer(&body).unwrap_or_else(|| "（模型未返回可用回答）".to_string());

    // 6. 持久化对话（仅指定 KB 时；全局问答无归属，不落库）
    if !kb_id.is_empty() {
        let sources_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
        let tokens = usage.as_ref().map(|u| u.total_tokens as i64).unwrap_or(0);
        let _ = repo
            .add_conversation(kb_id, "user", query, None, Some(chat_model), 0)
            .await;
        let _ = repo
            .add_conversation(kb_id, "assistant", &answer, Some(&sources_json), Some(chat_model), tokens)
            .await;
    }

    Ok(RagAnswer {
        answer,
        sources: results,
        usage: usage.map(|u| RagUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }),
    })
}

/// 经 `proxy::handle_request`（密钥分发）的非流式 chat 调用。
/// 返回 `(响应体, Token 用量)`；无可用密钥 / 被安全策略阻断 / 渠道失败 / 上游非 2xx 时返回错误。
async fn chat_completion(
    db: &Arc<Repository>,
    app: &AppHandle,
    api_key: &ApiKey,
    model: &str,
    results: &[SearchResult],
    query: &str,
    history: &[ConversationMessage],
    context_limit_override: Option<u64>,
) -> Result<(serde_json::Value, Option<TokenUsage>), String> {
    // 上下文上限：请求级覆盖优先，否则按「渠道 + 所选模型」解析。
    // 与代理内部分派同源（都是 get_enabled_channels + select_channels），取首项作估算基准。
    let context_limit = match context_limit_override {
        Some(v) if v > 0 => v,
        _ => {
            let enabled = db
                .get_enabled_channels()
                .await
                .map_err(|e| format!("读取启用渠道失败: {e}"))?;
            let ordered = Dispatcher::select_channels(&enabled, model);
            resolve_context_limit(ordered.first(), model)
        }
    };

    // 构建 prompt（含三级降级）
    let (prompt, _) = fit_context(results, query, history, context_limit);
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "你是一个基于知识库内容回答问题的助手。回答要基于提供的知识库内容，无法确定时明确说明。"
            },
            { "role": "user", "content": prompt },
        ],
        "stream": false,
    });

    // 经代理转发（安全扫描 + 渠道调度 + 重试 + 配额 + 日志），mode="rag"
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    let result = proxy::handle_request(db, app, &api_key.id, &api_key.name, body, false, Some(body_str), None, "rag")
        .await
        .map_err(|(code, msg)| format!("RAG 调用失败（{code}）：{msg}"))?;

    if !(200..300).contains(&result.status) {
        return Err(format!("RAG 上游返回 {}: {}", result.status, result.body));
    }
    Ok((result.body, result.usage))
}

/// 选择一个启用状态的 API 密钥用于内部 RAG 转发。
/// 优先：`allowed_models` 为空（不限模型）或包含所选模型；否则回退到任意启用密钥。
async fn select_api_key(db: &Arc<Repository>, model: &str) -> Result<ApiKey, String> {
    let keys = db
        .get_all_api_keys()
        .await
        .map_err(|e| format!("读取 API 密钥失败: {e}"))?;
    let enabled: Vec<ApiKey> = keys.into_iter().filter(|k| k.status == 1).collect();
    if enabled.is_empty() {
        return Err("没有可用的 API 密钥（请在「密钥」页创建一个启用状态的密钥）".to_string());
    }
    let model_lower = model.to_lowercase();
    let preferred = enabled.iter().find(|k| {
        let allowed: Vec<String> = serde_json::from_str(&k.allowed_models).unwrap_or_default();
        allowed.is_empty() || allowed.iter().any(|m| m.to_lowercase() == model_lower)
    });
    Ok(preferred.unwrap_or(&enabled[0]).clone())
}

/// 从 OpenAI 兼容响应抽取 `choices[0].message.content`。
fn extract_answer(body: &serde_json::Value) -> Option<String> {
    body.get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
}

/// 拼装上下文（文档 §3.5 格式）：`[来源: {filename} (相似度: {score:.2})]\n{content}`。
fn build_context(results: &[SearchResult]) -> String {
    let mut out = String::new();
    for (i, r) in results.iter().enumerate() {
        out.push_str(&format!(
            "[来源: {} (相似度: {:.2})]\n{}\n",
            r.filename, r.score, r.content
        ));
        if i + 1 < results.len() {
            out.push_str("---\n");
        }
    }
    out
}

/// 拼装 RAG prompt：系统指令 + 知识库内容 + 对话历史 + 问题。
fn build_rag_prompt(context: &str, query: &str, history: &[ConversationMessage]) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push("请根据下面的知识库内容回答用户问题。".to_string());
    if !context.is_empty() {
        parts.push(format!("## 知识库内容\n{context}"));
    }
    if !history.is_empty() {
        let hist = history
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("## 对话历史\n{hist}"));
    }
    parts.push(format!("## 问题\n{query}"));
    parts.join("\n\n")
}

/// 三级降级（文档 §3.3）：预算 = `context_limit * 70%`（30% 留给回答）。
/// 返回 `(最终 prompt, 保留 chunk 数)`。保底至少保留 1 个 chunk（极端单 chunk 超限时不强裁）。
fn fit_context(
    results: &[SearchResult],
    query: &str,
    history: &[ConversationMessage],
    context_limit: u64,
) -> (String, usize) {
    let budget = (context_limit * 70 / 100).max(1) as usize;

    // L0：全量
    let full_ctx = build_context(results);
    let full_prompt = build_rag_prompt(&full_ctx, query, history);
    if splitter::token_count(&full_prompt) <= budget {
        return (full_prompt, results.len());
    }

    // L1：逐个丢弃最低分 chunk（至少保留 1 个）
    let mut kept: Vec<SearchResult> = results.to_vec();
    while kept.len() > 1 {
        let min_idx = kept
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i);
        match min_idx {
            Some(i) => kept.remove(i),
            None => break,
        };
        let prompt = build_rag_prompt(&build_context(&kept), query, history);
        if splitter::token_count(&prompt) <= budget {
            return (prompt, kept.len());
        }
    }

    // L2：历史截断到最近 2 条
    let trimmed_history: Vec<ConversationMessage> =
        history.iter().rev().take(2).rev().cloned().collect();
    let prompt2 = build_rag_prompt(&build_context(&kept), query, &trimmed_history);
    if splitter::token_count(&prompt2) <= budget {
        return (prompt2, kept.len());
    }

    // L3：只保留 top-3 chunk、无历史、简化 prompt
    let mut final_res: Vec<SearchResult> = kept.into_iter().take(3).collect();
    let mut final_prompt = format!(
        "根据以下内容简要回答问题：\n{}\n\n问题：{query}",
        build_context(&final_res)
    );
    while final_res.len() > 1 && splitter::token_count(&final_prompt) > budget {
        final_res.pop();
        final_prompt = format!(
            "根据以下内容简要回答问题：\n{}\n\n问题：{query}",
            build_context(&final_res)
        );
    }
    (final_prompt, final_res.len())
}

/// 上下文上限解析链（用户确认：按「渠道 + 所选模型」具体配置）：
/// 1. `channel.config` JSON 的 `context_limits[model]`（数字，model 键名大小写不敏感：先精确后小写）
/// 2. `channel.config` JSON 的 `context_limit`（数字，渠道级默认）
/// 3. 环境变量 `YEAPI_KB_CONTEXT_LIMIT`
/// 4. 兜底 `32768`
fn resolve_context_limit(channel: Option<&Channel>, model: &str) -> u64 {
    if let Some(ch) = channel {
        let config: serde_json::Value =
            serde_json::from_str(&ch.config).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = config.as_object() {
            if let Some(limits) = obj.get("context_limits").and_then(|v| v.as_object()) {
                // 精确匹配优先
                if let Some(n) = limits.get(model).and_then(|v| v.as_u64()) {
                    return n;
                }
                // 大小写不敏感回退
                let model_lower = model.to_lowercase();
                for (k, v) in limits {
                    if k.to_lowercase() == model_lower {
                        if let Some(n) = v.as_u64() {
                            return n;
                        }
                    }
                }
            }
            if let Some(n) = obj.get("context_limit").and_then(|v| v.as_u64()) {
                return n;
            }
        }
    }
    if let Ok(v) = std::env::var(CONTEXT_LIMIT_ENV) {
        if let Ok(n) = v.trim().parse::<u64>() {
            if n > 0 {
                return n;
            }
        }
    }
    DEFAULT_CONTEXT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_with_config(config: &str) -> Channel {
        Channel {
            id: "ch1".into(),
            name: "ch1".into(),
            channel_type: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: "sk-test".into(),
            models: "[]".into(),
            status: 1,
            priority: 1,
            weight: 1,
            config: config.into(),
            model_mapping: "{}".into(),
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
            last_test_at: None,
            last_test_ok: None,
        }
    }

    fn sr(id: &str, score: f32, content: &str) -> SearchResult {
        SearchResult {
            chunk_id: id.into(),
            doc_id: "d".into(),
            filename: "f.md".into(),
            content: content.into(),
            score,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_resolve_context_limit_model_specific() {
        let ch = channel_with_config(
            r#"{"context_limits": {"deepseek-v4-flash": 100000}, "context_limit": 50000}"#,
        );
        assert_eq!(resolve_context_limit(Some(&ch), "deepseek-v4-flash"), 100000);
        // 未在 context_limits 中 → 落到 context_limit
        assert_eq!(resolve_context_limit(Some(&ch), "deepseek-v4-pro"), 50000);
    }

    #[test]
    fn test_resolve_context_limit_case_insensitive() {
        let ch = channel_with_config(r#"{"context_limits": {"DeepSeek-V4-Flash": 88888}}"#);
        assert_eq!(resolve_context_limit(Some(&ch), "deepseek-v4-flash"), 88888);
    }

    #[test]
    fn test_resolve_context_limit_no_channel_positive() {
        // 无渠道 → env 或兜底，均为正数
        assert!(resolve_context_limit(None, "any-model") > 0);
    }

    #[test]
    fn test_build_context_and_prompt() {
        let results = vec![sr("a", 0.9, "内容A"), sr("b", 0.8, "内容B")];
        let ctx = build_context(&results);
        assert!(ctx.contains("[来源: f.md (相似度: 0.90)]"));
        assert!(ctx.contains("内容A"));
        assert!(ctx.contains("---"));

        let prompt = build_rag_prompt(&ctx, "问题X", &[]);
        assert!(prompt.contains("## 知识库内容"));
        assert!(prompt.contains("## 问题"));
        assert!(prompt.contains("问题X"));
    }

    #[test]
    fn test_fit_context_l0_under_budget_keeps_all() {
        let results = vec![sr("a", 0.9, "短内容")];
        let (prompt, kept) = fit_context(&results, "问", &[], 100_000);
        assert_eq!(kept, 1);
        assert!(prompt.contains("短内容"));
    }

    #[test]
    fn test_fit_context_degrades_within_budget() {
        let results: Vec<SearchResult> = (0..50)
            .map(|i| sr(&format!("c{i}"), (50 - i) as f32, &"知识".repeat(500)))
            .collect();
        let history = vec![ConversationMessage {
            role: "user".into(),
            content: "你好".into(),
        }];
        let (prompt, kept) = fit_context(&results, "问题", &history, 2000);
        let budget = 2000 * 70 / 100;
        assert!(kept >= 1 && kept < results.len());
        assert!(splitter::token_count(&prompt) <= budget);
    }
}
