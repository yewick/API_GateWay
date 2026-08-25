//! 轻量级 HNSW 向量索引（单层简化实现）：构建 / 搜索 / 持久化 / 增量更新。
//!
//! 桌面级知识库（几百～几万 chunk）无需完整多层级 HNSW 与聚类训练，单层贪心图
//! 即可在近似 O(log n) 量级完成余弦相似度最近邻检索。节点 `id` 是「位置」而非
//! chunk_id，位置→chunk_id 的映射由 [`IndexStore::ids`] 持久化保证可恢复。

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 最近邻搜索结果（模块内类型，勿与 `models::SearchResult` 混淆）。
/// 检索端（retriever / hybrid_search）下一阶段接入，先作为库级原语预留。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    /// 节点位置（对应 [`IndexStore::ids`] 的下标）
    pub id: usize,
    /// 余弦距离（0 = 完全相同，越小越近）
    pub distance: f32,
}

/// 一个索引节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexNode {
    /// 位置 id（映射到 chunk 序列）
    pub id: usize,
    pub vector: Vec<f32>,
    /// 邻居节点在 `nodes` 中的下标
    pub neighbours: Vec<usize>,
}

/// 轻量级 HNSW 索引（单层）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswIndex {
    pub nodes: Vec<IndexNode>,
    pub max_m: usize,
    pub ef_search: usize,
    pub ef_construction: usize,
    pub dim: usize,
    pub entry_point: usize,
    pub initialized: bool,
}

impl Default for HnswIndex {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            max_m: 16,
            ef_search: 64,
            ef_construction: 200,
            dim: 0,
            entry_point: 0,
            initialized: false,
        }
    }
}

/// 余弦距离 `1 - cos`；零向量视为最远（1.0），结果 clamp 到 [0, 2]。
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 0.0 || nb <= 0.0 {
        return 1.0;
    }
    let cos = dot / (na.sqrt() * nb.sqrt());
    (1.0 - cos).clamp(0.0, 2.0)
}

impl HnswIndex {
    /// 增量插入一个节点（位置 id + 向量）。这是增量更新的机制：新 chunk 向量化后
    /// 追加即可，无需重建整个索引。维度与已有索引不一致时返回错误。
    pub fn insert(&mut self, id: usize, vector: Vec<f32>) -> Result<(), String> {
        if vector.is_empty() {
            return Err("空向量无法插入索引".to_string());
        }
        if self.dim == 0 {
            self.dim = vector.len();
        } else if self.dim != vector.len() {
            return Err(format!(
                "向量维度不一致：索引 {dim} vs 传入 {got}",
                dim = self.dim,
                got = vector.len()
            ));
        }

        let new_idx = self.nodes.len();
        self.nodes.push(IndexNode {
            id,
            vector,
            neighbours: Vec::new(),
        });

        if new_idx == 0 {
            self.entry_point = 0;
            self.initialized = true;
            return Ok(());
        }

        // 从入口贪心搜索近邻（用于连边）
        let query = self.nodes[new_idx].vector.clone();
        let candidates = self.search_layer(&query, self.ef_construction);

        // 与 top-max_m 近邻双向连边
        let neighbours: Vec<usize> = candidates.iter().map(|(i, _)| *i).take(self.max_m).collect();
        for &n in &neighbours {
            self.nodes[new_idx].neighbours.push(n);
            self.nodes[n].neighbours.push(new_idx);
        }
        // 邻居度超上限 → 剪枝（保留最近的 max_m*2 个）
        for &n in &neighbours {
            if self.nodes[n].neighbours.len() > self.max_m * 2 {
                self.prune_connections(n);
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// 批量构建（带进度回调 `cb(已处理, 总数)`）。
    pub fn build_with_progress<F>(
        &mut self,
        items: &[(usize, Vec<f32>)],
        mut cb: F,
    ) -> Result<(), String>
    where
        F: FnMut(usize, usize),
    {
        let total = items.len();
        for (i, (id, vector)) in items.iter().enumerate() {
            self.insert(*id, vector.clone())?;
            if i % 50 == 0 || i + 1 == total {
                cb(i + 1, total);
            }
        }
        Ok(())
    }

    /// 最近邻搜索：返回按距离升序的 top-k。
    #[allow(dead_code)] // 预留：检索端（retriever / hybrid_search）后续接入
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchResult> {
        if self.nodes.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let ef = self.ef_search.max(top_k).max(1);
        self.search_layer(query, ef)
            .into_iter()
            .take(top_k)
            .map(|(id, distance)| SearchResult { id, distance })
            .collect()
    }

    /// 单层 best-first 搜索。`candidates` 用小顶堆（最近候选先出），`results` 用
    /// 大顶堆（最差距离在顶）做早停；`ef` 限制扩展宽度。返回 `(节点下标, 距离)`。
    fn search_layer(&self, query: &[f32], ef: usize) -> Vec<(usize, f32)> {
        if self.nodes.is_empty() {
            return Vec::new();
        }
        let ef = ef.max(1);
        let entry = self.entry_point.min(self.nodes.len() - 1);

        let mut visited: HashSet<usize> = HashSet::new();
        // 距离非负且有限（[0,2]），用 to_bits() 单调编码为 u32 供堆排序（f32 无 Ord）。
        let mut candidates: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();
        let mut results: BinaryHeap<(u32, usize)> = BinaryHeap::new();

        let ed = cosine_distance(query, &self.nodes[entry].vector).to_bits();
        candidates.push(Reverse((ed, entry)));
        results.push((ed, entry));
        visited.insert(entry);

        while let Some(Reverse((db, idx))) = candidates.pop() {
            // 最近候选已比结果集最差的还远，且结果集已满 → 早停
            if let Some(&(worst, _)) = results.peek() {
                if db > worst && results.len() >= ef {
                    break;
                }
            }
            for &n in &self.nodes[idx].neighbours {
                if !visited.insert(n) {
                    continue;
                }
                let nd = cosine_distance(query, &self.nodes[n].vector).to_bits();
                let better = match results.peek() {
                    Some(&(worst, _)) => results.len() < ef || nd < worst,
                    None => true,
                };
                if better {
                    candidates.push(Reverse((nd, n)));
                    results.push((nd, n));
                    if results.len() > ef {
                        results.pop();
                    }
                }
            }
        }

        let mut out: Vec<(usize, f32)> = results
            .into_iter()
            .map(|(b, i)| (i, f32::from_bits(b)))
            .collect();
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        out
    }

    /// 剪枝：节点邻居数超 `max_m * 2` 时，只保留最近的 `max_m * 2` 个。
    fn prune_connections(&mut self, idx: usize) {
        let vector = self.nodes[idx].vector.clone();
        let neighbours = self.nodes[idx].neighbours.clone();
        if neighbours.len() <= self.max_m * 2 {
            return;
        }
        let mut scored: Vec<(usize, f32)> = neighbours
            .iter()
            .map(|&n| (n, cosine_distance(&vector, &self.nodes[n].vector)))
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        scored.truncate(self.max_m * 2);
        self.nodes[idx].neighbours = scored.into_iter().map(|(n, _)| n).collect();
    }

    /// 序列化到文件（bincode）。
    #[allow(dead_code)] // 预留：单索引直接读写（检索端接入后启用）
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = bincode::serialize(self).map_err(|e| format!("索引序列化失败: {e}"))?;
        std::fs::write(path, bytes).map_err(|e| format!("索引写入失败: {e}"))?;
        Ok(())
    }

    /// 从文件反序列化。
    #[allow(dead_code)] // 预留：单索引直接读写（检索端接入后启用）
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("索引读取失败: {e}"))?;
        bincode::deserialize(&bytes).map_err(|e| format!("索引反序列化失败: {e}"))
    }
}

/// 索引落盘封装：`ids[i]` 为位置 i 对应的 chunk_id。
/// 索引节点 `id` 是「位置」，`ids` 持久化位置→chunk_id 映射，使搜索结果可回查。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStore {
    pub ids: Vec<String>,
    pub index: HnswIndex,
}

impl IndexStore {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = bincode::serialize(self).map_err(|e| format!("索引序列化失败: {e}"))?;
        std::fs::write(path, bytes).map_err(|e| format!("索引写入失败: {e}"))?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("索引读取失败: {e}"))?;
        bincode::deserialize(&bytes).map_err(|e| format!("索引反序列化失败: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vec<f32> {
        vec![x, y]
    }

    #[test]
    fn test_cosine_distance() {
        assert!(cosine_distance(&v(1.0, 0.0), &v(1.0, 0.0)).abs() < 1e-6);
        assert!((cosine_distance(&v(1.0, 0.0), &v(0.0, 1.0)) - 1.0).abs() < 1e-6);
        // 零向量视为最远
        assert_eq!(cosine_distance(&v(0.0, 0.0), &v(1.0, 0.0)), 1.0);
    }

    #[test]
    fn test_build_and_search() {
        let mut idx = HnswIndex::default();
        let items = vec![
            (0usize, v(1.0, 0.0)),
            (1, v(0.0, 1.0)),
            (2, v(1.0, 1.0)),
            (3, v(-1.0, 0.0)),
        ];
        idx.build_with_progress(&items, |_, _| {}).unwrap();
        let hits = idx.search(&v(1.0, 0.0), 2);
        assert_eq!(hits.len(), 2);
        // 完全匹配的向量必为最近邻
        assert_eq!(hits[0].id, 0);
        assert!(hits[0].distance <= hits[1].distance);
    }

    #[test]
    fn test_search_sorted_ascending() {
        let mut idx = HnswIndex::default();
        for (i, vec) in [v(1.0, 0.0), v(0.9, 0.1), v(0.0, 1.0), v(0.1, 0.9)]
            .into_iter()
            .enumerate()
        {
            idx.insert(i, vec).unwrap();
        }
        let hits = idx.search(&v(1.0, 0.0), 4);
        assert_eq!(hits.len(), 4);
        for w in hits.windows(2) {
            assert!(w[0].distance <= w[1].distance);
        }
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut idx = HnswIndex::default();
        idx.build_with_progress(&[(0usize, v(1.0, 0.0)), (1, v(0.0, 1.0))], |_, _| {})
            .unwrap();
        let dir = std::env::temp_dir().join(format!("hnsw_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("idx.bin");
        idx.save(&path).unwrap();
        let loaded = HnswIndex::load(&path).unwrap();
        assert_eq!(idx.search(&v(1.0, 0.0), 2), loaded.search(&v(1.0, 0.0), 2));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_incremental_insert() {
        let mut idx = HnswIndex::default();
        idx.insert(0, v(1.0, 0.0)).unwrap();
        assert_eq!(idx.search(&v(1.0, 0.0), 1)[0].id, 0);
        idx.insert(1, v(0.0, 1.0)).unwrap();
        assert_eq!(idx.search(&v(0.0, 1.0), 1)[0].id, 1);
        assert_eq!(idx.nodes.len(), 2);
    }

    #[test]
    fn test_dim_mismatch_rejected() {
        let mut idx = HnswIndex::default();
        idx.insert(0, vec![1.0, 2.0]).unwrap();
        assert!(idx.insert(1, vec![1.0, 2.0, 3.0]).is_err());
    }

    #[test]
    fn test_index_store_ids_mapping() {
        let mut idx = HnswIndex::default();
        idx.build_with_progress(&[(0usize, v(1.0, 0.0)), (1, v(0.0, 1.0))], |_, _| {})
            .unwrap();
        let store = IndexStore {
            ids: vec!["c1".into(), "c2".into()],
            index: idx,
        };
        let hits = store.index.search(&v(1.0, 0.0), 1);
        assert_eq!(store.ids[hits[0].id], "c1");
    }
}
