//! 内存向量存储：brute-force 扫描，仅供开发/测试或小数据量使用。

use super::*;
use crate::error::DbError;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

struct Collection {
    dim: usize,
    distance: Distance,
    records: HashMap<String, Record>,
}

#[derive(Default)]
pub struct MemoryVectorStore {
    inner: Arc<RwLock<HashMap<String, Collection>>>,
}

impl MemoryVectorStore {
    pub fn new() -> Self { Self::default() }
}

#[async_trait]
impl VectorStore for MemoryVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec<'_>) -> Result<()> {
        let mut g = self.inner.write();
        g.entry(spec.name.to_string()).or_insert_with(|| Collection {
            dim: spec.dim,
            distance: spec.distance,
            records: HashMap::new(),
        });
        Ok(())
    }

    async fn drop_collection(&self, name: &str) -> Result<()> {
        self.inner.write().remove(name);
        Ok(())
    }

    async fn upsert(&self, collection: &str, records: &[Record]) -> Result<()> {
        let mut g = self.inner.write();
        let c = g.get_mut(collection).ok_or_else(||
            DbError::Other(format!("collection `{collection}` not found")))?;
        for r in records {
            if r.vector.len() != c.dim {
                return Err(DbError::Other(format!(
                    "dim mismatch: expected {}, got {}", c.dim, r.vector.len())));
            }
            c.records.insert(r.id.clone(), r.clone());
        }
        Ok(())
    }

    async fn delete(&self, collection: &str, ids: &[String]) -> Result<()> {
        let mut g = self.inner.write();
        if let Some(c) = g.get_mut(collection) {
            for id in ids { c.records.remove(id); }
        }
        Ok(())
    }

    async fn search(&self, collection: &str, query: Query<'_>) -> Result<Vec<Match>> {
        let g = self.inner.read();
        let c = g.get(collection).ok_or_else(||
            DbError::Other(format!("collection `{collection}` not found")))?;
        if query.vector.len() != c.dim {
            return Err(DbError::Other("query dim mismatch".into()));
        }

        let distance = c.distance;
        let qvec = query.vector;
        let filter = query.filter;
        let top_k = query.top_k.max(1);

        // 1. 过滤 + 计分；启用 vector-parallel 时用 rayon 并行
        let scored: Vec<(f32, &Record)> = {
            let iter = c.records.values().filter(|r| match filter {
                None => true,
                Some(f) => f.iter().all(|(k, v)| r.metadata.get(k) == Some(v)),
            });

            #[cfg(feature = "vector-parallel")]
            {
                use rayon::prelude::*;
                let v: Vec<&Record> = iter.collect();
                v.into_par_iter()
                    .map(|r| (score(distance, qvec, &r.vector), r))
                    .collect()
            }
            #[cfg(not(feature = "vector-parallel"))]
            {
                iter.map(|r| (score(distance, qvec, &r.vector), r)).collect()
            }
        };

        // 2. top-k 用 BinaryHeap 求最大 k 个，O(n log k) 而非 O(n log n)
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        // 维护一个最小堆（堆顶是当前 top-k 中最小分），超过 k 就 pop
        let mut heap: BinaryHeap<Reverse<HeapEntry>> = BinaryHeap::with_capacity(top_k + 1);
        for (sc, r) in scored {
            heap.push(Reverse(HeapEntry { score: sc, id: r.id.clone(), record: r }));
            if heap.len() > top_k {
                heap.pop();
            }
        }

        // 3. 输出按分数降序
        let mut hits: Vec<Match> = heap.into_iter()
            .map(|Reverse(e)| Match {
                id: e.id,
                score: e.score,
                metadata: e.record.metadata.clone(),
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(hits)
    }

    fn backend_name(&self) -> &'static str { "memory" }
}

struct HeapEntry<'a> {
    score: f32,
    id: String,
    record: &'a Record,
}

impl<'a> PartialEq for HeapEntry<'a> {
    fn eq(&self, other: &Self) -> bool { self.score == other.score }
}
impl<'a> Eq for HeapEntry<'a> {}
impl<'a> PartialOrd for HeapEntry<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl<'a> Ord for HeapEntry<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.partial_cmp(&other.score).unwrap_or(std::cmp::Ordering::Equal)
    }
}

fn score(d: Distance, a: &[f32], b: &[f32]) -> f32 {
    match d {
        Distance::Cosine => cosine_similarity(a, b),
        Distance::Dot => a.iter().zip(b).map(|(x, y)| x * y).sum(),
        // 转成相似度：距离越小越好 -> 取负
        Distance::L2 => {
            let s: f32 = a.iter().zip(b).map(|(x, y)| { let d = x - y; d * d }).sum();
            -s.sqrt()
        }
    }
}
