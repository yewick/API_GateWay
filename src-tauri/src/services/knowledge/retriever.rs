//! 检索层：HNSW 向量检索 + FTS5 关键词检索 + 混合加权融合 + 符号过滤。
//!
//! 不依赖 `AppHandle`：索引通过 `kb_index_meta.index_path`（DB 已存绝对路径）加载；
//! 结果统一为 [`models::SearchResult`]，`score` 语义为「越大越相关」。

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

use super::index::{cosine_distance, IndexStore};
use super::models::{ChunkMeta, SearchResult};
use super::repository::KbRepository;

/// 向量检索权重（默认 0.7 / 0.3）
pub const VECTOR_WEIGHT: f32 = 0.7;
pub const KEYWORD_WEIGHT: f32 = 0.3;

/// 载入索引：`status == "ready"` 且 `index_path` 存在时加载，否则 `None`（触发线性回退）。
pub async fn load_index(repo: &KbRepository, kb_id: &str) -> Option<IndexStore> {
    let meta = repo.get_index_meta(kb_id).await.ok()??;
    if meta.status != "ready" {
        return None;
    }
    let path = meta.index_path?;
    IndexStore::load(Path::new(&path)).ok()
}

/// 富化映射：`chunk_id → ChunkMeta`、`doc_id → filename`。
async fn build_maps(
    repo: &KbRepository,
    kb_id: &str,
) -> (HashMap<String, ChunkMeta>, HashMap<String, String>) {
    let metas = repo.get_chunks_meta(kb_id).await.unwrap_or_default();
    let docs = repo.get_documents(kb_id).await.unwrap_or_default();

    let mut chunk_map = HashMap::new();
    for m in metas {
        chunk_map.insert(m.id.clone(), m);
    }
    let mut doc_map = HashMap::new();
    for d in docs {
        doc_map.insert(d.id, d.filename);
    }
    (chunk_map, doc_map)
}

/// 把 `chunk_id → SearchResult`（解析 metadata JSON，并以列为准回填 symbol 信息）。
fn to_result(
    chunk_id: &str,
    chunk_map: &HashMap<String, ChunkMeta>,
    doc_map: &HashMap<String, String>,
    score: f32,
) -> Option<SearchResult> {
    let meta = chunk_map.get(chunk_id)?;
    let filename = doc_map.get(&meta.doc_id).cloned().unwrap_or_default();

    let mut metadata: serde_json::Value =
        serde_json::from_str(&meta.metadata).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = metadata.as_object_mut() {
        if let Some(k) = &meta.symbol_kind {
            obj.insert("symbol_kind".to_string(), serde_json::Value::String(k.clone()));
        }
        if let Some(n) = &meta.symbol_name {
            obj.insert("symbol_name".to_string(), serde_json::Value::String(n.clone()));
        }
    } else {
        metadata = serde_json::json!({
            "symbol_name": meta.symbol_name,
            "symbol_kind": meta.symbol_kind,
        });
    }

    Some(SearchResult {
        chunk_id: chunk_id.to_string(),
        doc_id: meta.doc_id.clone(),
        filename,
        content: meta.content.clone(),
        score,
        metadata,
    })
}

/// 把 `(chunk_id, score)` 序列富化为 `SearchResult`（去空、截断 top_k）。
fn finish(
    scored: &[(String, f32)],
    chunk_map: &HashMap<String, ChunkMeta>,
    doc_map: &HashMap<String, String>,
    top_k: usize,
) -> Vec<SearchResult> {
    scored
        .iter()
        .take(top_k)
        .filter_map(|(id, score)| to_result(id, chunk_map, doc_map, *score))
        .collect()
}

/// 向量检索：优先 HNSW（维度匹配时），否则线性余弦回退。
/// score = 余弦相似度（[0,1]，越大越相关）。
pub async fn vector_search(
    repo: &KbRepository,
    kb_id: &str,
    query_emb: &[f32],
    top_k: usize,
) -> Result<Vec<SearchResult>, String> {
    let (chunk_map, doc_map) = build_maps(repo, kb_id).await;

    if let Some(store) = load_index(repo, kb_id).await {
        if store.index.dim == query_emb.len() && !store.index.nodes.is_empty() {
            let mut scored: Vec<(String, f32)> = Vec::new();
            for hit in store.index.search(query_emb, top_k) {
                if let Some(id) = store.ids.get(hit.id) {
                    scored.push((id.clone(), (1.0 - hit.distance).max(0.0)));
                }
            }
            return Ok(finish(&scored, &chunk_map, &doc_map, top_k));
        }
    }

    // 线性回退：逐条算余弦相似度
    let pairs = repo
        .get_chunks_with_embeddings(kb_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut linear: Vec<(String, f32)> = pairs
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(id, v)| (id, (1.0 - cosine_distance(query_emb, &v)).max(0.0)))
        .collect();
    linear.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    linear.truncate(top_k);
    Ok(finish(&linear, &chunk_map, &doc_map, top_k))
}

/// FTS5 关键词检索。score `= 1 / (1 + rank)`（bm25 越小越相关 → 单调有界 (0,1]）。
pub async fn fts5_search(
    repo: &KbRepository,
    kb_id: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let hits = repo
        .search_fts(kb_id, trimmed, top_k as i64)
        .await
        .map_err(|e| e.to_string())?;
    let (chunk_map, doc_map) = build_maps(repo, kb_id).await;
    let scored: Vec<(String, f32)> = hits
        .iter()
        .map(|h| (h.chunk_id.clone(), (1.0 / (1.0 + h.rank)) as f32))
        .collect();
    Ok(finish(&scored, &chunk_map, &doc_map, top_k))
}

/// 混合检索：并行取向量与关键词各 `top_k * 2`，按 chunk_id 加权合并取 top_k。
pub async fn hybrid_search(
    repo: &KbRepository,
    kb_id: &str,
    query: &str,
    query_emb: &[f32],
    top_k: usize,
    vector_weight: f32,
    keyword_weight: f32,
) -> Result<Vec<SearchResult>, String> {
    let fetch_k = (top_k * 2).max(8);
    let (vec_res, fts_res) = tokio::try_join!(
        vector_search(repo, kb_id, query_emb, fetch_k),
        fts5_search(repo, kb_id, query, fetch_k),
    )?;
    Ok(merge_hybrid(vec_res, fts_res, vector_weight, keyword_weight, top_k))
}

/// 全局搜索：遍历所有知识库做混合检索，合并按 score 排序取 top_k。
/// `mcp_only` 时只检索 `mcp_enabled == 1` 的知识库。
pub async fn search_all(
    repo: &KbRepository,
    query: &str,
    query_emb: &[f32],
    top_k: usize,
    mcp_only: bool,
) -> Result<Vec<SearchResult>, String> {
    let kbs = repo.get_all_kbs().await.map_err(|e| e.to_string())?;
    let mut all: Vec<SearchResult> = Vec::new();
    for kb in kbs {
        if mcp_only && kb.mcp_enabled != 1 {
            continue;
        }
        if let Ok(mut res) = hybrid_search(
            repo,
            &kb.id,
            query,
            query_emb,
            top_k,
            VECTOR_WEIGHT,
            KEYWORD_WEIGHT,
        )
        .await
        {
            all.append(&mut res);
        }
    }
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    all.truncate(top_k);
    Ok(all)
}

/// 纯函数：按 chunk_id 加权合并向量与关键词结果，返回按合并分降序的 top_k。
pub fn merge_hybrid(
    vec_results: Vec<SearchResult>,
    fts_results: Vec<SearchResult>,
    vector_weight: f32,
    keyword_weight: f32,
    top_k: usize,
) -> Vec<SearchResult> {
    let mut merged: HashMap<String, (f32, SearchResult)> = HashMap::new();
    for r in vec_results {
        let entry = merged.entry(r.chunk_id.clone()).or_insert_with(|| (0.0, r.clone()));
        entry.0 += r.score * vector_weight;
    }
    for r in fts_results {
        let entry = merged.entry(r.chunk_id.clone()).or_insert_with(|| (0.0, r.clone()));
        entry.0 += r.score * keyword_weight;
    }
    let mut out: Vec<SearchResult> = merged
        .into_values()
        .map(|(score, mut r)| {
            r.score = score;
            r
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    out.truncate(top_k);
    out
}

/// 纯函数：按 `metadata["symbol_kind"]` 过滤结果（大小写不敏感；空 kind = 不过滤）。
pub fn filter_by_symbol(results: Vec<SearchResult>, symbol_kind: &str) -> Vec<SearchResult> {
    let kind = symbol_kind.trim().to_lowercase();
    if kind.is_empty() {
        return results;
    }
    results
        .into_iter()
        .filter(|r| {
            r.metadata
                .get("symbol_kind")
                .and_then(|v| v.as_str())
                .map(|k| k.to_lowercase() == kind)
                .unwrap_or(false)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(id: &str, score: f32) -> SearchResult {
        SearchResult {
            chunk_id: id.to_string(),
            doc_id: format!("doc-{id}"),
            filename: format!("{id}.md"),
            content: id.to_string(),
            score,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_merge_hybrid_weights_and_dedup() {
        let vec = vec![sr("a", 0.9), sr("b", 0.5)];
        let fts = vec![sr("a", 0.8), sr("c", 0.7)];
        let merged = merge_hybrid(vec, fts, 0.7, 0.3, 10);
        // a 合并：0.9*0.7 + 0.8*0.3 = 0.63 + 0.24 = 0.87；c：0.21；b：0.35
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].chunk_id, "a");
        assert!((merged[0].score - 0.87).abs() < 1e-5);
    }

    #[test]
    fn test_merge_hybrid_truncates_top_k() {
        let vec = vec![sr("a", 0.9), sr("b", 0.8)];
        let fts = vec![sr("c", 0.7)];
        let merged = merge_hybrid(vec, fts, 0.7, 0.3, 2);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_filter_by_symbol() {
        let mut a = sr("a", 0.9);
        a.metadata = serde_json::json!({ "symbol_kind": "function" });
        let mut b = sr("b", 0.8);
        b.metadata = serde_json::json!({ "symbol_kind": "class" });
        let all = vec![a, b];
        let fns = filter_by_symbol(all.clone(), "Function");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].chunk_id, "a");
        // 空 kind 不过滤
        assert_eq!(filter_by_symbol(all, "").len(), 2);
    }

    #[test]
    fn test_filter_by_symbol_missing_metadata_dropped() {
        let a = sr("a", 0.9); // 无 symbol_kind
        let filtered = filter_by_symbol(vec![a], "function");
        assert!(filtered.is_empty());
    }
}
