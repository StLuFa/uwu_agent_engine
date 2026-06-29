//! Errors for `uwu_event_mesh`.

use thiserror::Error;

use super::type_id::TypeId;

#[derive(Debug, Error)]
pub enum EventMeshError {
    #[error("invalid topic: {0}")]
    InvalidTopic(String),

    #[error("invalid topic pattern: {0}")]
    InvalidPattern(String),

    #[error("serialize event failed: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("mesh is closed")]
    Closed,

    #[error("subscriber lagged or dropped")]
    SubscriberGone,

    #[error("no event store attached to this mesh")]
    NoStore,

    #[error("event store io error: {0}")]
    Io(#[from] std::io::Error),

    /// Unknown type id at cross-process boundary.
    /// The registry rejected this event — it may be from an untrusted source
    /// or a newer version of a peer that we don't know about yet.
    #[error("unknown event type: {0}")]
    UnknownType(TypeId),

    /// Deserialization failed for a *registered* type.
    /// This means the payload bytes don't match the expected schema — likely
    /// a version mismatch or corrupted data.
    #[error("deserialize {0} failed: {1}")]
    Deserialize(String, String),
}

pub type Result<T> = std::result::Result<T, EventMeshError>;
