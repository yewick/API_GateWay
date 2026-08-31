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

/// 向量/关键词权重（默认 0.7 / 0.3）。
/// 混合检索已改用 RRF（见 [`merge_hybrid_scored`]），这两个常量仅作为 API 兼容参数继续传递、
/// 不再参与融合计算；保留以维持 `search_all*` 的默认参数签名不变。
pub const VECTOR_WEIGHT: f32 = 0.7;
pub const KEYWORD_WEIGHT: f32 = 0.3;

/// 带分项评分的检索结果：保留向量/关键词原始分，供 [`models::RetrievalDetail`] 展示。
pub struct ScoredSearchResult {
    pub result: SearchResult,
    /// 向量相似度（原始分，未加权；仅 vector / hybrid 命中时存在）
    pub vector_score: Option<f32>,
    /// 关键词相关度（原始分，未加权；仅 keyword / hybrid 命中时存在）
    pub keyword_score: Option<f32>,
}

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
        parent_id: meta.parent_id.clone(),
        parent_content: None,
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

/// 关键词检索（`search_mode = "keyword"` 的别名包装）。
pub async fn keyword_only_search(
    repo: &KbRepository,
    kb_id: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>, String> {
    fts5_search(repo, kb_id, query, top_k).await
}

/// 混合检索（带分项评分）：并行取向量与关键词各 `top_k * 2`，加权合并返回 `ScoredSearchResult`。
pub async fn hybrid_search_with_details(
    repo: &KbRepository,
    kb_id: &str,
    query: &str,
    query_emb: &[f32],
    top_k: usize,
    vector_weight: f32,
    keyword_weight: f32,
) -> Result<Vec<ScoredSearchResult>, String> {
    let fetch_k = (top_k * 2).max(8);
    let (vec_res, fts_res) = tokio::try_join!(
        vector_search(repo, kb_id, query_emb, fetch_k),
        fts5_search(repo, kb_id, query, fetch_k),
    )?;
    Ok(merge_hybrid_scored(vec_res, fts_res, vector_weight, keyword_weight, top_k))
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
    Ok(hybrid_search_with_details(repo, kb_id, query, query_emb, top_k, vector_weight, keyword_weight)
        .await?
        .into_iter()
        .map(|s| s.result)
        .collect())
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
    Ok(search_all_with_details(
        repo,
        query,
        Some(query_emb),
        top_k,
        mcp_only,
        VECTOR_WEIGHT,
        KEYWORD_WEIGHT,
        "hybrid",
    )
    .await?
    .into_iter()
    .map(|s| s.result)
    .collect())
}

/// 全局搜索（带分项评分）：遍历所有知识库，按 `search_mode` 分派检索，合并排序取 top_k。
/// `query_emb`：vector / hybrid 模式需要；keyword 模式可为 `None`。
/// `mcp_only` 时只检索 `mcp_enabled == 1` 的知识库；未知 `search_mode` 回退 hybrid。
pub async fn search_all_with_details(
    repo: &KbRepository,
    query: &str,
    query_emb: Option<&[f32]>,
    top_k: usize,
    mcp_only: bool,
    vector_weight: f32,
    keyword_weight: f32,
    search_mode: &str,
) -> Result<Vec<ScoredSearchResult>, String> {
    let kbs = repo.get_all_kbs().await.map_err(|e| e.to_string())?;
    let mut all: Vec<ScoredSearchResult> = Vec::new();
    for kb in kbs {
        if mcp_only && kb.mcp_enabled != 1 {
            continue;
        }
        let scored: Vec<ScoredSearchResult> = match search_mode {
            "vector" => match query_emb {
                Some(emb) => vector_search(repo, &kb.id, emb, top_k)
                    .await
                    .map(|rs| {
                        rs.into_iter()
                            .map(|r| {
                                let s = r.score;
                                ScoredSearchResult {
                                    result: r,
                                    vector_score: Some(s),
                                    keyword_score: None,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                None => Vec::new(),
            },
            "keyword" => keyword_only_search(repo, &kb.id, query, top_k)
                .await
                .map(|rs| {
                    rs.into_iter()
                        .map(|r| {
                            let s = r.score;
                            ScoredSearchResult {
                                result: r,
                                vector_score: None,
                                keyword_score: Some(s),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => match query_emb {
                Some(emb) => hybrid_search_with_details(
                    repo,
                    &kb.id,
                    query,
                    emb,
                    top_k,
                    vector_weight,
                    keyword_weight,
                )
                .await
                .unwrap_or_default(),
                None => Vec::new(),
            },
        };
        all.extend(scored);
    }
    all.sort_by(|a, b| b.result.score.partial_cmp(&a.result.score).unwrap_or(Ordering::Equal));
    all.truncate(top_k);
    Ok(all)
}

/// 纯函数：按 chunk_id 加权合并向量与关键词结果，返回按合并分降序的 top_k。
/// 仅由单测使用（生产路径走 [`merge_hybrid_scored`]），故标注 `#[cfg(test)]`。
#[cfg(test)]
pub fn merge_hybrid(
    vec_results: Vec<SearchResult>,
    fts_results: Vec<SearchResult>,
    vector_weight: f32,
    keyword_weight: f32,
    top_k: usize,
) -> Vec<SearchResult> {
    merge_hybrid_scored(vec_results, fts_results, vector_weight, keyword_weight, top_k)
        .into_iter()
        .map(|s| s.result)
        .collect()
}

/// 纯函数：按 chunk_id 用 **RRF（Reciprocal Rank Fusion）** 合并向量与关键词结果，保留分项评分，
/// 返回按融合分降序的 top_k。
///
/// 与旧的「向量/关键词原始分加权求和」不同，RRF 只看各流内的**名次**（1-based），消除两种评分
/// 量纲差异带来的偏差：旧实现中纯关键词命中原始分上限仅 0.3，会被 5 个纯向量命中（上限 0.7）挤出
/// top_k；RRF 下「关键词第 1 名」恒胜过「向量第 1 名」，直接命中「精确关键词被挤掉」的症状。
///
/// - `result.score` = `1/(K_VEC + vec_rank) + 1/(K_KW + kw_rank)`（未命中记 0）；
/// - `K_KW=30 < K_VEC=60` 让关键词流每档贡献更大（轻微偏向精确关键词命中）；
/// - `vector_score` / `keyword_score` 保存各自原始分（未命中为 `None`），供前端检索调试；
/// - `vector_weight` / `keyword_weight` 仅为 API 兼容保留，RRF 下不参与计算。
pub fn merge_hybrid_scored(
    vec_results: Vec<SearchResult>,
    fts_results: Vec<SearchResult>,
    _vector_weight: f32,
    _keyword_weight: f32,
    top_k: usize,
) -> Vec<ScoredSearchResult> {
    const K_VEC: f32 = 60.0;
    const K_KW: f32 = 30.0;

    // 名次映射：各自流内 1-based；未命中不在表中
    let vec_rank: HashMap<String, usize> = vec_results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.chunk_id.clone(), i + 1))
        .collect();
    let kw_rank: HashMap<String, usize> = fts_results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.chunk_id.clone(), i + 1))
        .collect();

    struct Acc {
        result: Option<SearchResult>,
        vec_raw: Option<f32>,
        kw_raw: Option<f32>,
    }
    let mut merged: HashMap<String, Acc> = HashMap::new();
    for r in vec_results {
        let entry = merged.entry(r.chunk_id.clone()).or_insert_with(|| Acc {
            result: None,
            vec_raw: None,
            kw_raw: None,
        });
        if entry.result.is_none() {
            entry.result = Some(r.clone());
        }
        entry.vec_raw = Some(r.score);
    }
    for r in fts_results {
        let entry = merged.entry(r.chunk_id.clone()).or_insert_with(|| Acc {
            result: None,
            vec_raw: None,
            kw_raw: None,
        });
        if entry.result.is_none() {
            entry.result = Some(r.clone());
        }
        entry.kw_raw = Some(r.score);
    }

    let mut out: Vec<ScoredSearchResult> = merged
        .into_iter()
        .map(|(chunk_id, acc)| {
            let rrf = match (vec_rank.get(&chunk_id), kw_rank.get(&chunk_id)) {
                (Some(vr), Some(kr)) => 1.0 / (K_VEC + *vr as f32) + 1.0 / (K_KW + *kr as f32),
                (Some(vr), None) => 1.0 / (K_VEC + *vr as f32),
                (None, Some(kr)) => 1.0 / (K_KW + *kr as f32),
                (None, None) => 0.0,
            };
            let mut result = acc.result.unwrap_or_else(|| SearchResult {
                chunk_id: chunk_id.clone(),
                doc_id: String::new(),
                filename: String::new(),
                content: String::new(),
                score: 0.0,
                metadata: serde_json::Value::Null,
                parent_id: None,
                parent_content: None,
            });
            result.score = rrf;
            ScoredSearchResult {
                result,
                vector_score: acc.vec_raw,
                keyword_score: acc.kw_raw,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.result
            .score
            .partial_cmp(&a.result.score)
            .unwrap_or(Ordering::Equal)
    });
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
            parent_id: None,
            parent_content: None,
        }
    }

    #[test]
    fn test_merge_hybrid_rrf_ranks_and_dedup() {
        let vec = vec![sr("a", 0.9), sr("b", 0.5)];
        let fts = vec![sr("a", 0.8), sr("c", 0.7)];
        let merged = merge_hybrid(vec, fts, 0.7, 0.3, 10);
        // RRF：a 双路第 1（1/61 + 1/31 ≈ 0.04865）；c 关键词第 2（1/32 ≈ 0.03125）；b 向量第 2（1/62 ≈ 0.01613）
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].chunk_id, "a");
        assert_eq!(merged[1].chunk_id, "c");
        assert_eq!(merged[2].chunk_id, "b");
        assert!(merged[0].score > merged[1].score && merged[1].score > merged[2].score);
    }

    #[test]
    fn test_merge_hybrid_rrf_keyword_rank1_beats_vector_rank1() {
        // 回归根因 1：纯关键词命中第 1 名应胜过纯向量命中第 1 名（旧加权实现下关键词会被挤出）
        let vec = vec![sr("vec1", 0.95), sr("vec2", 0.9), sr("vec3", 0.85)];
        let fts = vec![sr("kw1", 0.9)];
        let merged = merge_hybrid(vec, fts, 0.7, 0.3, 5);
        // kw1 关键词第 1：1/31 ≈ 0.03226；vec1 向量第 1：1/61 ≈ 0.01639
        assert_eq!(merged[0].chunk_id, "kw1");
        assert!(merged[0].score > merged[1].score);
    }

    #[test]
    fn test_merge_hybrid_truncates_top_k() {
        let vec = vec![sr("a", 0.9), sr("b", 0.8)];
        let fts = vec![sr("c", 0.7)];
        let merged = merge_hybrid(vec, fts, 0.7, 0.3, 2);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_hybrid_scored_keeps_component_scores() {
        let vec = vec![sr("a", 0.9), sr("b", 0.5)];
        let fts = vec![sr("a", 0.8), sr("c", 0.7)];
        let merged = merge_hybrid_scored(vec, fts, 0.7, 0.3, 10);
        assert_eq!(merged.len(), 3);

        // a：双路第 1，score = 1/61 + 1/31
        let a = merged.iter().find(|s| s.result.chunk_id == "a").unwrap();
        assert!((a.result.score - (1.0 / 61.0 + 1.0 / 31.0)).abs() < 1e-6);
        assert!((a.vector_score.unwrap() - 0.9).abs() < 1e-5);
        assert!((a.keyword_score.unwrap() - 0.8).abs() < 1e-5);

        // b：只在向量路，keyword_score 为 None，score = 1/62
        let b = merged.iter().find(|s| s.result.chunk_id == "b").unwrap();
        assert!(b.keyword_score.is_none());
        assert!((b.result.score - 1.0 / 62.0).abs() < 1e-6);

        // c：只在关键词路，vector_score 为 None，score = 1/32
        let c = merged.iter().find(|s| s.result.chunk_id == "c").unwrap();
        assert!(c.vector_score.is_none());
        assert!((c.result.score - 1.0 / 32.0).abs() < 1e-6);
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
