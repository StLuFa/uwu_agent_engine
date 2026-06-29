//! pgvector 后端：复用现有 PostgreSQL Pool。
//!
//! 表结构（由 [`PgVectorStore`] 自动建立）：
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS <name> (
//!   id        TEXT PRIMARY KEY,
//!   embedding vector(<dim>) NOT NULL,
//!   metadata  JSONB NOT NULL DEFAULT '{}'::jsonb
//! );
//! ```
//!
//! 性能优化点：
//! - `upsert` 使用单条多值 `INSERT ... VALUES (...), (...) ...`，按 batch 切分（每批 1000 条），
//!   避免 round-trip。
//! - 支持按集合配置 `Distance`（cosine / l2 / dot），search 时用对应运算符。
//! - 提供 [`PgVectorStore::create_hnsw_index`] / [`create_ivfflat_index`] 显式建索引。

use super::*;
use crate::error::DbError;
use crate::sql::DbPool;
use parking_lot::RwLock;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;

pub struct PgVectorStore {
    pool: Arc<DbPool>,
    /// collection -> distance（建表时记录，search 自动选择运算符）
    distances: Arc<RwLock<HashMap<String, Distance>>>,
}

impl PgVectorStore {
    pub fn new(pool: Arc<DbPool>) -> Result<Self> {
        pool.as_postgres()?;
        Ok(Self { pool, distances: Default::default() })
    }

    fn pg(&self) -> &sqlx::PgPool {
        self.pool.as_postgres().expect("postgres pool")
    }

    fn distance_of(&self, collection: &str) -> Distance {
        self.distances.read().get(collection).copied().unwrap_or(Distance::Cosine)
    }

    /// 创建 HNSW 索引（pgvector 0.5+）。
    pub async fn create_hnsw_index(&self, collection: &str, m: u32, ef_construction: u32) -> Result<()> {
        validate_ident(collection)?;
        let op = index_op(self.distance_of(collection));
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {collection}_hnsw \
             ON {collection} USING hnsw (embedding {op}) \
             WITH (m = {m}, ef_construction = {ef_construction})"
        );
        sqlx::query(&sql).execute(self.pg()).await?;
        Ok(())
    }

    /// 创建 IVFFlat 索引（pgvector 0.4+）。
    pub async fn create_ivfflat_index(&self, collection: &str, lists: u32) -> Result<()> {
        validate_ident(collection)?;
        let op = index_op(self.distance_of(collection));
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {collection}_ivf \
             ON {collection} USING ivfflat (embedding {op}) WITH (lists = {lists})"
        );
        sqlx::query(&sql).execute(self.pg()).await?;
        Ok(())
    }
}

fn op_str(d: Distance) -> &'static str {
    match d {
        Distance::Cosine => "<=>",
        Distance::L2 => "<->",
        Distance::Dot => "<#>",
    }
}

fn index_op(d: Distance) -> &'static str {
    match d {
        Distance::Cosine => "vector_cosine_ops",
        Distance::L2 => "vector_l2_ops",
        Distance::Dot => "vector_ip_ops",
    }
}

fn vector_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 8 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&x.to_string());
    }
    s.push(']');
    s
}

const UPSERT_BATCH: usize = 1000;

#[async_trait]
impl VectorStore for PgVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec<'_>) -> Result<()> {
        validate_ident(spec.name)?;
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {name} (\
                id TEXT PRIMARY KEY, \
                embedding vector({dim}) NOT NULL, \
                metadata JSONB NOT NULL DEFAULT '{{}}'::jsonb)",
            name = spec.name, dim = spec.dim,
        );
        sqlx::query(&sql).execute(self.pg()).await?;
        self.distances.write().insert(spec.name.to_string(), spec.distance);
        Ok(())
    }

    async fn drop_collection(&self, name: &str) -> Result<()> {
        validate_ident(name)?;
        let sql = format!("DROP TABLE IF EXISTS {name}");
        sqlx::query(&sql).execute(self.pg()).await?;
        self.distances.write().remove(name);
        Ok(())
    }

    /// 批量 upsert：按 1000 条/批拼成多值 INSERT，单事务提交。
    async fn upsert(&self, collection: &str, records: &[Record]) -> Result<()> {
        validate_ident(collection)?;
        if records.is_empty() { return Ok(()); }

        let mut tx = self.pg().begin().await?;
        for chunk in records.chunks(UPSERT_BATCH) {
            // 构造 $1,$2,$3 / $4,$5,$6 / ...
            let mut placeholders = String::new();
            for i in 0..chunk.len() {
                if i > 0 { placeholders.push(','); }
                let base = i * 3;
                placeholders.push_str(&format!(
                    "(${},${}::vector,${}::jsonb)",
                    base + 1, base + 2, base + 3
                ));
            }
            let sql = format!(
                "INSERT INTO {collection} (id, embedding, metadata) VALUES {placeholders} \
                 ON CONFLICT (id) DO UPDATE SET embedding = EXCLUDED.embedding, metadata = EXCLUDED.metadata"
            );
            let mut q = sqlx::query(&sql);
            for r in chunk {
                let meta = serde_json::Value::Object(r.metadata.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect());
                q = q.bind(r.id.clone()).bind(vector_literal(&r.vector)).bind(meta);
            }
            q.execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn delete(&self, collection: &str, ids: &[String]) -> Result<()> {
        validate_ident(collection)?;
        if ids.is_empty() { return Ok(()); }
        let sql = format!("DELETE FROM {collection} WHERE id = ANY($1)");
        sqlx::query(&sql).bind(ids).execute(self.pg()).await?;
        Ok(())
    }

    async fn search(&self, collection: &str, query: Query<'_>) -> Result<Vec<Match>> {
        validate_ident(collection)?;
        let dist = self.distance_of(collection);
        let op = op_str(dist);
        let (filter_sql, filter_value) = match query.filter {
            Some(f) if !f.is_empty() => (
                "WHERE metadata @> $2".to_string(),
                Some(serde_json::Value::Object(f.clone().into_iter().collect())),
            ),
            _ => (String::new(), None),
        };
        let sql = format!(
            "SELECT id, metadata, embedding {op} $1::vector AS distance \
             FROM {collection} {filter_sql} \
             ORDER BY embedding {op} $1::vector ASC \
             LIMIT {limit}",
            limit = query.top_k as i64,
        );
        let mut q = sqlx::query(&sql).bind(vector_literal(query.vector));
        if let Some(v) = filter_value { q = q.bind(v); }
        let rows = q.fetch_all(self.pg()).await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            let meta: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::json!({}));
            let dist_val: f64 = row.try_get("distance")?;
            // 距离 -> 相似度分数
            let score = match dist {
                Distance::Cosine => 1.0 - dist_val as f32,    // cosine distance ∈ [0,2]，score ∈ [-1,1]
                Distance::L2 => -(dist_val as f32),           // 越小越相似 -> 取负
                Distance::Dot => -(dist_val as f32),          // pgvector <#> 返回 -inner_product
            };
            let metadata = match meta {
                serde_json::Value::Object(m) => m.into_iter().collect(),
                _ => Default::default(),
            };
            out.push(Match { id, score, metadata });
        }
        Ok(out)
    }

    fn backend_name(&self) -> &'static str { "pgvector" }
}

fn validate_ident(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 63 {
        return Err(DbError::Other(format!("invalid identifier `{s}`")));
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(DbError::Other(format!("invalid identifier `{s}`")));
    }
    Ok(())
}
