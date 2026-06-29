//! Type-safe identifiers for the event mesh ecosystem.
//!
//! - [`TypeId`]: namespaced event type identifier, used by [`crate::core::serialized_envelope::SerializedEnvelope`]
//!   and [`crate::core::type_registry::TypeRegistry`] for safe cross-process deserialization.
//! - [`CorrelationId`]: ties together all envelopes belonging to one logical flow / task.
//! - [`ReplayId`]: marks replay events so consumers can skip side-effects.

use serde::{Deserialize, Serialize};

/// Fully-qualified event type name.
///
/// # Examples
///
/// ```
/// # use uwu_event_mesh::core::type_id::TypeId;
/// let tid = TypeId::new("state", "snapshot");
/// assert_eq!(tid.to_string(), "state.snapshot");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId {
    /// Logical domain / crate name (e.g. `"state"`, `"task"`, `"guard"`).
    pub domain: String,
    /// Event name within the domain (e.g. `"snapshot"`, `"created"`, `"violation"`).
    pub event: String,
}

impl TypeId {
    /// Create a new `TypeId`.
    ///
    /// # Panics (debug)
    ///
    /// Panics if `domain` or `event` is empty or contains `.`.
    #[track_caller]
    pub fn new(domain: impl Into<String>, event: impl Into<String>) -> Self {
        let d = domain.into();
        let e = event.into();
        debug_assert!(!d.is_empty(), "domain must not be empty");
        debug_assert!(!e.is_empty(), "event must not be empty");
        debug_assert!(!d.contains('.'), "domain must not contain '.'");
        debug_assert!(!e.contains('.'), "event must not contain '.'");
        Self { domain: d, event: e }
    }

    /// Parse from `"domain.event"` string.
    pub fn parse(s: &str) -> Option<Self> {
        let (domain, event) = s.split_once('.')?;
        if domain.is_empty() || event.is_empty() {
            return None;
        }
        Some(Self {
            domain: domain.to_string(),
            event: event.to_string(),
        })
    }
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.domain, self.event)
    }
}

/// Unique identifier for a logical flow / task — ties together all envelopes
/// belonging to one causal chain across topics.
pub type CorrelationId = String;

/// Marker attached to replayed events so consumers can distinguish
/// live events from historical replays and skip side-effects.
pub type ReplayId = String;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_id_roundtrip() {
        let tid = TypeId::new("state", "snapshot");
        assert_eq!(tid.to_string(), "state.snapshot");
        let parsed = TypeId::parse("state.snapshot").unwrap();
        assert_eq!(tid, parsed);
    }

    #[test]
    fn type_id_parse_invalid() {
        assert!(TypeId::parse("").is_none());
        assert!(TypeId::parse("no_dot").is_none());
        assert!(TypeId::parse(".leading").is_none());
        assert!(TypeId::parse("trailing.").is_none());
    }
}
