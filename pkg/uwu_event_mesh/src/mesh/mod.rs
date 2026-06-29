//! In-process event mesh broker.
//!
//! All publish/subscribe goes through [`EventMesh`]. Subscribers receive
//! events that match their topic pattern, in publish order, via a bounded
//! async channel. Events are delivered as `Arc<Envelope>` so fan-out is
//! allocation-free.
//!
//! Optional persistent backing: attach an [`crate::EventStore`] via
//! [`EventMesh::with_store`] and use [`EventMesh::replay`] to re-deliver
//! historical events.
//!
//! Per-subscription [`BackpressurePolicy`] controls slow-consumer behavior.
//!
//! For cross-process event streaming, use [`flow_handle::FlowHandle`] /
//! [`flow_handle::FlowReceiver`] with [`crate::core::serialized_envelope::SerializedEnvelope`]
//! and [`crate::core::type_registry::TypeRegistry`].

mod broker;
mod dedup;
pub mod flow_handle;

mod ring;
mod subscriber;

pub use broker::{DEFAULT_BUFFER, DEFAULT_DEDUP_WINDOW, EventMesh, EventMeshBuilder};

pub use flow_handle::{
    FlowChannel, FlowHandle, FlowReceiver, CONSOLIDATION_CAPACITY, MAIN_CAPACITY,
    MONITORING_CAPACITY, SYSTEM_CAPACITY,
};

pub use subscriber::{
    AckMode, BackpressurePolicy, GroupStrategy, Requeue, SubId, SubscribeOptions, Subscription,
    TryRecvError,
};
