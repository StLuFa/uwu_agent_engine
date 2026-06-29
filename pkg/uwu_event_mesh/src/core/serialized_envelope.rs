//! Cross-process-safe serialized envelope.
//!
//! Unlike [`super::envelope::Envelope`] which carries `serde_json::Value`,
//! `SerializedEnvelope` carries `payload_bytes: Vec<u8>` — the pre-serialized
//! payload. This allows the envelope to traverse process boundaries (NATS,
//! Kafka, gRPC) without losing type information.
//!
//! Safe deserialization is enforced by [`super::type_registry::TypeRegistry`]:
//! unknown `TypeId` values are rejected at the boundary, preventing
//! deserialization-based injection attacks.
//!
//! # Conversion
//!
//! ```
//! # use uwu_event_mesh::core::envelope::Envelope;
//! # use uwu_event_mesh::core::serialized_envelope::SerializedEnvelope;
//! # use uwu_event_mesh::core::type_id::TypeId;
//! # use serde_json::json;
//! # use crate::uwu_event_mesh::core::topic::Topic;
//! // Envelope → SerializedEnvelope
//! let topic = Topic::new("test.x").unwrap();
//! let env = Envelope::new(&topic, json!({"key": "val"}))
//!     .with_type_id(TypeId::new("test", "x"));
//! let ser = SerializedEnvelope::from_envelope(&env).unwrap();
//!
//! // SerializedEnvelope → Envelope (requires TypeRegistry for typed safety,
//! // or the `unchecked` path for JSON passthrough).
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::error::{EventMeshError, Result};
use crate::core::metadata::EventMetadata;
use super::envelope::Envelope;
use super::type_id::{CorrelationId, ReplayId, TypeId};

/// Cross-process-safe event envelope.
///
/// Replaces `Envelope::payload: Value` with `payload_bytes: Vec<u8>`.
/// Construction is typed via `new::<T: Serialize>()`; deserialization
/// is gated by [`super::type_registry::TypeRegistry`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEnvelope {
    /// Fully-qualified event type. Required for cross-process deserialization.
    pub type_id: TypeId,
    /// Unique event id.
    pub id: Uuid,
    /// Topic this event is published to.
    pub topic: String,
    /// Event creation time (UTC).
    pub timestamp: DateTime<Utc>,

    /// Direct cause. None = root event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    /// Root of the causal chain.
    pub root_id: Uuid,
    /// OpenTelemetry-compatible trace id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,

    /// Flow / task correlation id.
    pub correlation_id: CorrelationId,
    /// Monotonic sequence number assigned by [`super::super::mesh::flow_handle::FlowHandle`].
    pub sequence_number: u64,
    /// If `Some`, this event is a replay — consumers SHOULD skip side-effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_id: Option<ReplayId>,

    /// Optional dedup key for idempotent publishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Optional time-to-live in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,

    /// Source cell / producer name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Pre-serialized payload bytes (JSON).
    pub payload_bytes: Vec<u8>,

    /// Producer metadata (wall-clock time, producer id, TTL).
    pub metadata: EventMetadata,

    /// Optional headers.
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

impl SerializedEnvelope {
    /// Create a new root `SerializedEnvelope` with a typed payload.
    ///
    /// Serializes `payload` to JSON bytes immediately.
    pub fn new<T: Serialize>(
        type_id: TypeId,
        topic: impl Into<String>,
        correlation_id: impl Into<CorrelationId>,
        producer_id: impl Into<String>,
        payload: &T,
    ) -> Result<Self> {
        let id = Uuid::new_v4();
        let topic = topic.into();
        let correlation_id = correlation_id.into();
        let payload_bytes = serde_json::to_vec(payload)?;
        Ok(Self {
            type_id,
            id,
            topic,
            timestamp: Utc::now(),
            parent_id: None,
            root_id: id,
            trace_id: None,
            correlation_id,
            sequence_number: 0,
            replay_id: None,
            idempotency_key: None,
            ttl_ms: None,
            source: None,
            payload_bytes,
            metadata: EventMetadata::new(producer_id),
            headers: Default::default(),
        })
    }

    /// Create a child envelope causally linked to `parent`.
    pub fn child_of<T: Serialize>(
        parent: &Self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<Self> {
        let id = Uuid::new_v4();
        let payload_bytes = serde_json::to_vec(payload)?;
        Ok(Self {
            type_id,
            id,
            topic: topic.into(),
            timestamp: Utc::now(),
            parent_id: Some(parent.id),
            root_id: parent.root_id,
            trace_id: parent.trace_id.clone(),
            correlation_id: parent.correlation_id.clone(),
            sequence_number: 0,
            replay_id: parent.replay_id.clone(),
            idempotency_key: None,
            ttl_ms: None,
            source: None,
            payload_bytes,
            metadata: EventMetadata::new(&parent.metadata.producer_id),
            headers: Default::default(),
        })
    }

    /// Deserialize payload as the expected type `T`.
    ///
    /// This is a convenience for when the caller already knows the type.
    /// For safe boundary deserialization with unknown types, use
    /// [`super::type_registry::TypeRegistry::deserialize`].
    pub fn deserialize_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_slice(&self.payload_bytes)?)
    }

    /// Convert from an in-process [`Envelope`]. The payload is serialized to
    /// bytes. Returns an error if `type_id` is not set on the source envelope.
    pub fn from_envelope(env: &Envelope) -> Result<Self> {
        let type_id = env
            .type_id
            .clone()
            .ok_or_else(|| EventMeshError::Serialize(
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Envelope::type_id is required to convert to SerializedEnvelope",
                ))
            ))?;
        let payload_bytes = serde_json::to_vec(&env.payload)?;
        Ok(Self {
            type_id,
            id: env.id,
            topic: env.topic.clone(),
            timestamp: env.timestamp,
            parent_id: env.parent_id,
            root_id: env.root_id,
            trace_id: env.trace_id.clone(),
            correlation_id: env
                .correlation_id
                .clone()
                .unwrap_or_else(|| env.id.to_string()),
            sequence_number: env.sequence_number,
            replay_id: env.replay_id.clone(),
            idempotency_key: env.idempotency_key.clone(),
            ttl_ms: env.ttl_ms,
            source: env.source.clone(),
            payload_bytes,
            metadata: EventMetadata::new(
                env.source.clone().unwrap_or_else(|| "unknown".into()),
            ),
            headers: env.headers.clone(),
        })
    }

    /// Convert back to an in-process [`Envelope`] (payload is deserialized
    /// as `Value` — the type-safety contract is already fulfilled by the
    /// boundary check in `TypeRegistry`).
    pub fn into_envelope(self) -> Result<Envelope> {
        let payload: serde_json::Value = serde_json::from_slice(&self.payload_bytes)?;
        Ok(Envelope {
            id: self.id,
            topic: self.topic,
            timestamp: self.timestamp,
            parent_id: self.parent_id,
            root_id: self.root_id,
            trace_id: self.trace_id,
            idempotency_key: self.idempotency_key,
            ttl_ms: self.ttl_ms,
            source: self.source,
            type_id: Some(self.type_id),
            correlation_id: Some(self.correlation_id),
            sequence_number: self.sequence_number,
            replay_id: self.replay_id,
            payload,
            headers: self.headers,
        })
    }

    // ---- builder methods ----

    pub fn with_sequence_number(mut self, seq: u64) -> Self {
        self.sequence_number = seq;
        self
    }

    pub fn with_replay_id(mut self, rid: impl Into<ReplayId>) -> Self {
        self.replay_id = Some(rid.into());
        self
    }

    pub fn with_correlation_id(mut self, cid: impl Into<CorrelationId>) -> Self {
        self.correlation_id = cid.into();
        self
    }

    pub fn with_idempotency_key(mut self, k: impl Into<String>) -> Self {
        self.idempotency_key = Some(k.into());
        self
    }

    pub fn with_ttl_ms(mut self, ttl: u64) -> Self {
        self.ttl_ms = Some(ttl);
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_trace(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    pub fn with_header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.insert(k.into(), v.into());
        self
    }

    /// True if this is a replay event.
    pub fn is_replay(&self) -> bool {
        self.replay_id.is_some()
    }

    /// True if the metadata TTL has elapsed.
    pub fn is_expired(&self) -> bool {
        self.metadata.is_expired()
            || self.ttl_ms.map_or(false, |ttl| {
                let elapsed = Utc::now()
                    .signed_duration_since(self.timestamp)
                    .num_milliseconds()
                    .max(0) as u64;
                elapsed > ttl
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        msg: String,
        count: u32,
    }

    #[test]
    fn roundtrip_typed() {
        let payload = TestPayload { msg: "hello".into(), count: 42 };
        let se = SerializedEnvelope::new(
            TypeId::new("test", "roundtrip"),
            "test.topic",
            "corr-1",
            "producer-1",
            &payload,
        )
        .unwrap();
        assert_eq!(se.type_id.to_string(), "test.roundtrip");
        assert_eq!(se.correlation_id, "corr-1");
        assert_eq!(se.sequence_number, 0);
        assert!(se.replay_id.is_none());

        let decoded: TestPayload = se.deserialize_payload().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn envelope_conversion_roundtrip() {
        let tid = TypeId::new("test", "conv");
        let topic = crate::core::topic::Topic::new("test.conv").unwrap();
        let env = Envelope::new(&topic, serde_json::json!({"x": 1}))
            .with_type_id(tid.clone())
            .with_correlation_id("corr-2")
            .with_sequence_number(7);

        let se = SerializedEnvelope::from_envelope(&env).unwrap();
        assert_eq!(se.type_id, tid);
        assert_eq!(se.sequence_number, 7);

        let env2 = se.into_envelope().unwrap();
        assert_eq!(env2.payload, serde_json::json!({"x": 1}));
        assert_eq!(env2.sequence_number, 7);
        assert_eq!(env2.correlation_id.as_deref(), Some("corr-2"));
    }

    #[test]
    fn child_inherits_correlation_and_replay() {
        let parent = SerializedEnvelope::new(
            TypeId::new("test", "parent"),
            "test.p",
            "corr-3",
            "prod",
            &TestPayload { msg: "p".into(), count: 1 },
        )
        .unwrap()
        .with_replay_id("replay-batch-1");

        let child = SerializedEnvelope::child_of(
            &parent,
            TypeId::new("test", "child"),
            "test.c",
            &TestPayload { msg: "c".into(), count: 2 },
        )
        .unwrap();

        assert_eq!(child.correlation_id, "corr-3");
        assert_eq!(child.replay_id.as_deref(), Some("replay-batch-1"));
        assert_eq!(child.parent_id, Some(parent.id));
        assert_eq!(child.root_id, parent.root_id);
    }
}
