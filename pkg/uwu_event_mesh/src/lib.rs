//! # uwu_event_mesh
//!
//! In-process **event mesh** with hierarchical topics, causal envelopes,
//! typed event sets, and pluggable persistence + replay. Designed as the
//! foundational pub/sub layer for the FlowMind double-mesh architecture
//! (event mesh + matrix mesh).
//!
//! ## cross-process safety
//!
//! - [`SerializedEnvelope`](core::serialized_envelope::SerializedEnvelope) replaces
//!   `Box<dyn Any>` for cross-process event transport.
//! - [`TypeRegistry`](core::type_registry::TypeRegistry) ensures only known types
//!   are deserialized at the process boundary.
//! - [`FlowHandle`](mesh::flow_handle::FlowHandle) provides four-channel
//!   (main/consolidation/monitoring/system) typed publishing with monotonic
//!   sequence numbers.

pub mod bridge;
pub mod core;
pub mod ext;
pub mod mesh;
pub mod store;

pub use bridge::{Bridge, ChannelBridge, ChannelBridgePair};
pub use core::{
    envelope::Envelope,
    error::{EventMeshError, Result},
    metadata::EventMetadata,
    serialized_envelope::SerializedEnvelope,
    topic::{Topic, TopicPattern},
    type_id::{CorrelationId, ReplayId, TypeId},
    type_registry::TypeRegistry,
};
pub use ext::{
    event_set::{EventKind, EventSet, TypedSubscription},
    filter::{EnvelopePredicate, Filter},
    idempotency::{DedupKey, IdempotencyStore, MemoryIdempotencyStore, process_idempotent},
};
pub use mesh::{
    AckMode, BackpressurePolicy, DEFAULT_BUFFER, DEFAULT_DEDUP_WINDOW, EventMesh, EventMeshBuilder,
    FlowChannel, FlowHandle, FlowReceiver, GroupStrategy, Requeue, SubId, SubscribeOptions,
    Subscription, TryRecvError, CONSOLIDATION_CAPACITY, MAIN_CAPACITY, MONITORING_CAPACITY,
    SYSTEM_CAPACITY,
};
pub use store::{
    EventStore, JsonlStore, JsonlStoreOptions, MemoryStore, ReplayFilter, SegmentedStore,
    SegmentedStoreOptions,
};

/// One-stop import for the most commonly used items.
///
/// ```
/// use uwu_event_mesh::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        AckMode, BackpressurePolicy, Bridge, ChannelBridge, ChannelBridgePair, DedupKey, Envelope,
        EventMesh, EventMeshError, EventMetadata, EventSet, EventStore, Filter, FlowChannel,
        FlowHandle, FlowReceiver, GroupStrategy, IdempotencyStore, JsonlStore,
        MemoryIdempotencyStore, MemoryStore, ReplayFilter, Requeue, Result, SegmentedStore,
        SerializedEnvelope, SubscribeOptions, Subscription, Topic, TopicPattern, TypeId,
        TypeRegistry,
    };
}
