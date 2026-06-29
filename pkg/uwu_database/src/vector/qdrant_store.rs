//! Qdrant 后端，使用官方 `qdrant-client` (gRPC)。

use super::*;
use crate::error::DbError;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, DeletePointsBuilder, Distance as QDistance, PointId, PointStruct,
    PointsIdsList, SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
    Condition, Filter, Value,
    value::Kind,
};
use qdrant_client::{Payload, Qdrant};

pub struct QdrantVectorStore {
    client: Qdrant,
}

impl QdrantVectorStore {
    pub fn new(url: &str, api_key: Option<String>) -> Result<Self> {
        let mut b = Qdrant::from_url(url);
        if let Some(k) = api_key { b = b.api_key(k); }
        let client = b.build().map_err(|e| DbError::Other(e.to_string()))?;
        Ok(Self { client })
    }

    pub fn from_client(client: Qdrant) -> Self { Self { client } }
}

fn to_qdistance(d: Distance) -> QDistance {
    match d {
        Distance::Cosine => QDistance::Cosine,
        Distance::L2 => QDistance::Euclid,
        Distance::Dot => QDistance::Dot,
    }
}

fn to_payload(meta: &std::collections::HashMap<String, serde_json::Value>) -> Payload {
    let obj: serde_json::Map<String, serde_json::Value> =
        meta.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    Payload::try_from(serde_json::Value::Object(obj)).unwrap_or_default()
}

fn json_value_from_qdrant(v: Value) -> serde_json::Value {
    match v.kind {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(b),
        Some(Kind::IntegerValue(i)) => serde_json::Value::from(i),
        Some(Kind::DoubleValue(d)) => serde_json::Number::from_f64(d)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s),
        Some(Kind::ListValue(l)) => serde_json::Value::Array(
            l.values.into_iter().map(json_value_from_qdrant).collect(),
        ),
        Some(Kind::StructValue(s)) => serde_json::Value::Object(
            s.fields.into_iter().map(|(k, v)| (k, json_value_from_qdrant(v))).collect(),
        ),
    }
}

fn from_payload(p: std::collections::HashMap<String, Value>)
    -> std::collections::HashMap<String, serde_json::Value>
{
    p.into_iter().map(|(k, v)| (k, json_value_from_qdrant(v))).collect()
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec<'_>) -> Result<()> {
        if let Ok(true) = self.client.collection_exists(spec.name).await {
            return Ok(());
        }
        self.client
            .create_collection(
                CreateCollectionBuilder::new(spec.name)
                    .vectors_config(VectorParamsBuilder::new(spec.dim as u64, to_qdistance(spec.distance))),
            )
            .await
            .map_err(|e| DbError::Other(e.to_string()))?;
        Ok(())
    }

    async fn drop_collection(&self, name: &str) -> Result<()> {
        self.client.delete_collection(name).await
            .map_err(|e| DbError::Other(e.to_string()))?;
        Ok(())
    }

    async fn upsert(&self, collection: &str, records: &[Record]) -> Result<()> {
        let points: Vec<PointStruct> = records.iter().map(|r| {
            PointStruct::new(r.id.clone(), r.vector.clone(), to_payload(&r.metadata))
        }).collect();
        self.client
            .upsert_points(UpsertPointsBuilder::new(collection, points))
            .await
            .map_err(|e| DbError::Other(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, collection: &str, ids: &[String]) -> Result<()> {
        let point_ids: Vec<PointId> = ids.iter().cloned().map(PointId::from).collect();
        let list = PointsIdsList { ids: point_ids };
        self.client
            .delete_points(DeletePointsBuilder::new(collection).points(list))
            .await
            .map_err(|e| DbError::Other(e.to_string()))?;
        Ok(())
    }

    async fn search(&self, collection: &str, query: Query<'_>) -> Result<Vec<Match>> {
        let mut req = SearchPointsBuilder::new(collection, query.vector.to_vec(), query.top_k as u64)
            .with_payload(true);
        if let Some(f) = query.filter {
            if !f.is_empty() {
                let conds: Vec<Condition> = f.iter().filter_map(|(k, v)| {
                    match v {
                        serde_json::Value::String(s) => Some(Condition::matches(k.as_str(), s.clone())),
                        serde_json::Value::Bool(b) => Some(Condition::matches(k.as_str(), *b)),
                        serde_json::Value::Number(n) => n.as_i64().map(|i| Condition::matches(k.as_str(), i)),
                        _ => None,
                    }
                }).collect();
                if !conds.is_empty() {
                    req = req.filter(Filter::must(conds));
                }
            }
        }
        let resp = self.client.search_points(req).await
            .map_err(|e| DbError::Other(e.to_string()))?;

        let out = resp.result.into_iter().map(|p| {
            let id = p.id.map(|pid| match pid.point_id_options {
                Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u)) => u,
                Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(n)) => n.to_string(),
                None => String::new(),
            }).unwrap_or_default();
            Match { id, score: p.score, metadata: from_payload(p.payload) }
        }).collect();
        Ok(out)
    }

    fn backend_name(&self) -> &'static str { "qdrant" }
}

#[allow(dead_code)]
fn _ensure_selector_compiles() {
    let _: Option<PointsIdsList> = None;
}
