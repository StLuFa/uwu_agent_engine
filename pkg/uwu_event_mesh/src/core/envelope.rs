//! Event envelope: every event flowing through the mesh is wrapped in this.
//!
//! Carries identity, causality (parent / root), idempotency, ttl,
//! cross-process type safety ([`TypeId`]), flow correlation
//! ([`CorrelationId`]), monotonic [`sequence_number`], and a
//! [`replay_id`] marker so consumers can skip side-effects during replay.
//!
//! For cross-process transport, use [`super::serialized_envelope::SerializedEnvelope`]
//! which replaces `payload: Value` with `payload_bytes: Vec<u8>` and couples
//! with [`super::type_registry::TypeRegistry`] for safe deserialization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::topic::Topic;
use super::type_id::{CorrelationId, ReplayId, TypeId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Unique event id.
    pub id: Uuid,
    /// Topic this event is published to.
    pub topic: String,
    /// Event creation time (UTC).
    pub timestamp: DateTime<Utc>,

    /// Direct cause of this event. None = root event.
    pub parent_id: Option<Uuid>,
    /// Root of the causal chain. Equals `id` for root events.
    pub root_id: Uuid,
    /// Optional trace id for cross-system tracing (OpenTelemetry compatible).
    pub trace_id: Option<String>,

    /// Optional dedup key. Two envelopes with the same key on the same topic
    /// within the dedup window are considered duplicates.
    pub idempotency_key: Option<String>,
    /// Optional time-to-live in milliseconds. None = no expiry.
    pub ttl_ms: Option<u64>,

    /// Source cell / producer name. Free-form.
    pub source: Option<String>,

    /// --- cross-process safety fields ---

    /// Fully-qualified event type (`"domain.event"`). Set by producers to
    /// enable safe cross-process deserialization via [`super::type_registry::TypeRegistry`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<TypeId>,
    /// Flow / task correlation id. All envelopes in one logical flow share the
    /// same correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<CorrelationId>,
    /// Monotonic sequence number assigned by [`super::super::mesh::flow_handle::FlowHandle`].
    /// 0 means "unassigned".
    #[serde(default)]
    pub sequence_number: u64,
    /// If `Some`, this envelope is a replay event. Consumers SHOULD skip
    /// side-effects when this is set. The value is the replay batch id
    /// for observability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_id: Option<ReplayId>,

    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,

    /// Optional headers (key/value metadata not part of payload).
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

impl Envelope {
    /// Build a new root envelope.
    pub fn new(topic: &Topic, payload: serde_json::Value) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            topic: topic.as_str().to_string(),
            timestamp: Utc::now(),
            parent_id: None,
            root_id: id,
            trace_id: None,
            idempotency_key: None,
            ttl_ms: None,
            source: None,
            type_id: None,
            correlation_id: None,
            sequence_number: 0,
            replay_id: None,
            payload,
            headers: Default::default(),
        }
    }

    /// Build a child envelope causally linked to `parent`.
    pub fn child_of(parent: &Envelope, topic: &Topic, payload: serde_json::Value) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            topic: topic.as_str().to_string(),
            timestamp: Utc::now(),
            parent_id: Some(parent.id),
            root_id: parent.root_id,
            trace_id: parent.trace_id.clone(),
            idempotency_key: None,
            ttl_ms: None,
            source: None,
            // Inherit correlation from parent so the whole flow is traceable.
            type_id: None,
            correlation_id: parent.correlation_id.clone(),
            sequence_number: 0,
            replay_id: parent.replay_id.clone(),
            payload,
            headers: Default::default(),
        }
    }

    // ---- builder methods ----

    pub fn with_type_id(mut self, tid: TypeId) -> Self {
        self.type_id = Some(tid);
        self
    }

    pub fn with_correlation_id(mut self, cid: impl Into<CorrelationId>) -> Self {
        self.correlation_id = Some(cid.into());
        self
    }

    pub fn with_sequence_number(mut self, seq: u64) -> Self {
        self.sequence_number = seq;
        self
    }

    pub fn with_replay_id(mut self, rid: impl Into<ReplayId>) -> Self {
        self.replay_id = Some(rid.into());
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
    pub fn with_idempotency_key(mut self, k: impl Into<String>) -> Self {
        self.idempotency_key = Some(k.into());
        self
    }
    pub fn with_ttl_ms(mut self, ttl: u64) -> Self {
        self.ttl_ms = Some(ttl);
        self
    }
    pub fn with_header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.insert(k.into(), v.into());
        self
    }

    /// True if `ttl_ms` is set and elapsed.
    pub fn is_expired(&self) -> bool {
        let Some(ttl) = self.ttl_ms else { return false };
        let now = Utc::now();
        let elapsed = now
            .signed_duration_since(self.timestamp)
            .num_milliseconds()
            .max(0) as u64;
        elapsed > ttl
    }

    /// True if this is a replay event (consumer should skip side-effects).
    pub fn is_replay(&self) -> bool {
        self.replay_id.is_some()
    }
}
