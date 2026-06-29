//! Type registry for safe cross-process deserialization.
//!
//! Every event type that flows across process boundaries must be registered
//! here before deserialization. Unknown `TypeId` values are rejected,
//! preventing deserialization-based injection attacks.
//!
//! # Usage
//!
//! ```ignore
//! let registry = TypeRegistry::new();
//! registry.register::<OrderCreated>("order", "created");
//!
//! // At the boundary — safe deserialization:
//! let result: Box<dyn Any + Send + Sync> = registry
//!     .deserialize(&envelope.type_id, &envelope.payload_bytes)?;
//! let order: &OrderCreated = result.downcast_ref().unwrap();
//! ```
//!
//! # Thread safety
//!
//! Registration is typically done at startup (single-threaded). Reads
//! (deserialization) are concurrent and lock-free after registration.

use std::any::{Any, TypeId as StdTypeId};
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::de::DeserializeOwned;

use crate::core::error::{EventMeshError, Result};
use super::type_id::TypeId;

/// A factory function that deserializes raw bytes into a boxed `Any`.
type DeserializeFn =
    Arc<dyn Fn(Vec<u8>) -> std::result::Result<Box<dyn Any + Send + Sync>, String> + Send + Sync>;

/// Registry of known event types for safe cross-process deserialization.
///
/// # Design
///
/// Each entry maps `TypeId` → `DeserializeFn`. At the process boundary,
/// incoming `SerializedEnvelope`s are checked against this registry:
///
/// - **Known type**: deserialize, downcast, dispatch.
/// - **Unknown type**: return `Error::UnknownType` — never attempt blind
///   deserialization.
///
/// The registry also records the Rust `std::any::TypeId` for each entry,
/// enabling `downcast_ref` after deserialization.
pub struct TypeRegistry {
    entries: RwLock<HashMap<TypeId, RegistryEntry>>,
}

struct RegistryEntry {
    deserializer: DeserializeFn,
    /// Stored for future `downcast_ref` verification after deserialization.
    #[allow(dead_code)]
    rust_type_id: StdTypeId,
    /// Human-readable Rust type name for diagnostics.
    type_name: &'static str,
}

impl TypeRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register a type under the given `domain` and `event` name.
    ///
    /// # Panics (debug)
    ///
    /// Panics if the same `TypeId` is registered twice (duplicate registration
    /// is a startup bug).
    #[track_caller]
    pub fn register<T: DeserializeOwned + Send + Sync + 'static>(
        &self,
        domain: impl Into<String>,
        event: impl Into<String>,
    ) {
        let type_id = TypeId::new(domain, event);
        let mut entries = self.entries.write();
        assert!(
            !entries.contains_key(&type_id),
            "duplicate TypeId registration: {type_id}"
        );
        entries.insert(
            type_id,
            RegistryEntry {
                deserializer: Arc::new(|bytes: Vec<u8>| {
                    serde_json::from_slice::<T>(&bytes)
                        .map(|v| Box::new(v) as Box<dyn Any + Send + Sync>)
                        .map_err(|e| e.to_string())
                }),
                rust_type_id: StdTypeId::of::<T>(),
                type_name: std::any::type_name::<T>(),
            },
        );
    }

    /// Convenience: register from a `TypeId` value directly.
    #[track_caller]
    pub fn register_with<T: DeserializeOwned + Send + Sync + 'static>(
        &self,
        type_id: TypeId,
    ) {
        let mut entries = self.entries.write();
        assert!(
            !entries.contains_key(&type_id),
            "duplicate TypeId registration: {type_id}"
        );
        entries.insert(
            type_id,
            RegistryEntry {
                deserializer: Arc::new(|bytes: Vec<u8>| {
                    serde_json::from_slice::<T>(&bytes)
                        .map(|v| Box::new(v) as Box<dyn Any + Send + Sync>)
                        .map_err(|e| e.to_string())
                }),
                rust_type_id: StdTypeId::of::<T>(),
                type_name: std::any::type_name::<T>(),
            },
        );
    }

    /// Safely deserialize an event payload.
    ///
    /// Returns:
    /// - `Ok(Box<dyn Any>)` if the type is registered and deserialization succeeds.
    /// - `Err(EventMeshError::UnknownType)` if the `TypeId` is not registered.
    /// - `Err(EventMeshError::Deserialize)` if deserialization fails.
    pub fn deserialize(
        &self,
        type_id: &TypeId,
        bytes: &[u8],
    ) -> Result<Box<dyn Any + Send + Sync>> {
        let entries = self.entries.read();
        let entry = entries
            .get(type_id)
            .ok_or_else(|| EventMeshError::UnknownType(type_id.clone()))?;
        (entry.deserializer)(bytes.to_vec())
            .map_err(|msg| EventMeshError::Deserialize(type_id.to_string(), msg))
    }

    /// Check whether a `TypeId` is registered.
    pub fn is_registered(&self, type_id: &TypeId) -> bool {
        self.entries.read().contains_key(type_id)
    }

    /// Number of registered types.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// True if no types are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// List all registered `TypeId`s (for debugging / metrics).
    pub fn registered_types(&self) -> Vec<TypeId> {
        self.entries.read().keys().cloned().collect()
    }

    /// Get the Rust type name for a registered `TypeId` (diagnostics).
    pub fn type_name(&self, type_id: &TypeId) -> Option<&'static str> {
        self.entries.read().get(type_id).map(|e| e.type_name)
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TypeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeRegistry")
            .field("count", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct OrderCreated {
        id: u64,
        amount: f64,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct OrderShipped {
        id: u64,
        tracking: String,
    }

    fn setup() -> TypeRegistry {
        let reg = TypeRegistry::new();
        reg.register::<OrderCreated>("order", "created");
        reg.register::<OrderShipped>("order", "shipped");
        reg
    }

    #[test]
    fn deserialize_registered_type() {
        let reg = setup();
        let payload = OrderCreated { id: 1, amount: 99.0 };
        let bytes = serde_json::to_vec(&payload).unwrap();
        let tid = TypeId::new("order", "created");

        let result = reg.deserialize(&tid, &bytes).unwrap();
        let order: &OrderCreated = result.downcast_ref().unwrap();
        assert_eq!(order, &payload);
    }

    #[test]
    fn reject_unknown_type() {
        let reg = setup();
        let bytes = serde_json::to_vec(&OrderCreated { id: 1, amount: 1.0 }).unwrap();
        let tid = TypeId::new("order", "unknown");

        match reg.deserialize(&tid, &bytes) {
            Err(EventMeshError::UnknownType(t)) => assert_eq!(t.event, "unknown"),
            other => panic!("expected UnknownType, got {other:?}"),
        }
    }

    #[test]
    fn reject_deserialize_mismatch() {
        let reg = setup();
        // Register as OrderCreated but pass OrderShipped bytes
        let payload = OrderShipped { id: 2, tracking: "SF".into() };
        let bytes = serde_json::to_vec(&payload).unwrap();
        let tid = TypeId::new("order", "created"); // wrong type!

        match reg.deserialize(&tid, &bytes) {
            Err(EventMeshError::Deserialize(..)) => {} // expected
            other => panic!("expected Deserialize error, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_registration_panics() {
        let reg = TypeRegistry::new();
        reg.register::<OrderCreated>("order", "created");
        // Second registration of same TypeId should panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reg.register::<OrderCreated>("order", "created");
        }));
        assert!(result.is_err());
    }

    #[test]
    fn list_registered_types() {
        let reg = setup();
        let types = reg.registered_types();
        assert_eq!(types.len(), 2);
        assert!(reg.is_registered(&TypeId::new("order", "created")));
        assert!(!reg.is_registered(&TypeId::new("order", "nope")));
    }
}
