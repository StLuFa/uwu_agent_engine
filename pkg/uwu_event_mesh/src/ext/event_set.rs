//! Event sets: typed groups of related events.
//!
//! An `EventSet` declares a set of event kinds that share a topic namespace
//! and provides type-safe publish/subscribe on top of the underlying mesh.
//!
//! Each event kind has:
//! - a sub-topic name (e.g. `"created"`, appended to the set's namespace)
//! - a Rust payload type (any `Serialize + DeserializeOwned`)
//!
//! ## Why?
//!
//! Raw `EventMesh` is JSON-typed by design (so events can flow across
//! languages / processes). `EventSet` gives a producer/consumer in the *same*
//! crate a typed facade so you don't sprinkle `serde_json::Value` everywhere.

use std::marker::PhantomData;
use std::sync::Arc;

use serde::{Serialize, de::DeserializeOwned};

use crate::core::envelope::Envelope;
use crate::core::error::Result;
use crate::mesh::{EventMesh, Subscription};
use crate::core::topic::{Topic, TopicPattern};

/// A declaration of one event kind within a set.
///
/// Created via [`EventSet::kind`].
pub struct EventKind<T> {
    namespace: String,
    name: &'static str,
    _marker: PhantomData<fn() -> T>,
}

impl<T> EventKind<T>
where
    T: Serialize + DeserializeOwned + Send + 'static,
{
    /// The full topic name for this kind (e.g. `flow.order.created`).
    pub fn topic_str(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }

    pub fn topic(&self) -> Topic {
        // Safe: namespace and name are pre-validated in `EventSet::new`/`kind`.
        Topic::new(self.topic_str()).expect("EventKind topic always valid")
    }

    /// Decode an envelope's payload into the typed value.
    pub fn decode(&self, env: &Envelope) -> Result<T> {
        Ok(serde_json::from_value(env.payload.clone())?)
    }
}

/// A typed group of events sharing a topic namespace.
///
/// ```ignore
/// let mesh = EventMesh::new();
/// let set = EventSet::new(&mesh, "flow.order").unwrap();
/// let created = set.kind::<OrderCreated>("created");
/// set.emit(&created, &OrderCreated { id: 1 }).await?;
/// ```
pub struct EventSet {
    mesh: EventMesh,
    namespace: String,
}

impl EventSet {
    pub fn new(mesh: &EventMesh, namespace: impl Into<String>) -> Result<Self> {
        let ns = namespace.into();
        // Validate namespace by attempting to build a topic with a trailing segment.
        let _ = Topic::new(format!("{ns}._validate_"))?;
        Ok(Self {
            mesh: mesh.clone(),
            namespace: ns,
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn mesh(&self) -> &EventMesh {
        &self.mesh
    }

    /// Declare an event kind under this set.
    pub fn kind<T>(&self, name: &'static str) -> EventKind<T>
    where
        T: Serialize + DeserializeOwned + Send + 'static,
    {
        // Validate eagerly so misuse fails at declaration site.
        let _ = Topic::new(format!("{}.{}", self.namespace, name))
            .expect("invalid event kind name");
        EventKind {
            namespace: self.namespace.clone(),
            name,
            _marker: PhantomData,
        }
    }

    /// Type-safe publish.
    pub async fn emit<T>(&self, kind: &EventKind<T>, payload: &T) -> Result<usize>
    where
        T: Serialize + DeserializeOwned + Send + 'static,
    {
        let topic = kind.topic();
        let value = serde_json::to_value(payload)?;
        self.mesh.emit(&topic, value).await
    }

    /// Publish a pre-built envelope retargeted at this kind's topic.
    /// Useful for forwarding causality (`Envelope::child_of`).
    pub async fn publish<T>(&self, kind: &EventKind<T>, mut env: Envelope) -> Result<usize>
    where
        T: Serialize + DeserializeOwned + Send + 'static,
    {
        env.topic = kind.topic_str();
        self.mesh.publish(env).await
    }

    /// Subscribe to a single kind. Returns a typed-decoding subscription.
    pub fn subscribe<T>(&self, kind: &EventKind<T>) -> TypedSubscription<T>
    where
        T: Serialize + DeserializeOwned + Send + 'static,
    {
        let pattern = TopicPattern::new(kind.topic_str())
            .expect("EventKind pattern always valid");
        TypedSubscription {
            inner: self.mesh.subscribe(pattern),
            _marker: PhantomData,
        }
    }

    /// Subscribe to *all* events in this set (`<namespace>.>`).
    pub fn subscribe_all(&self) -> Subscription {
        let pattern = TopicPattern::new(format!("{}.>", self.namespace))
            .expect("EventSet wildcard always valid");
        self.mesh.subscribe(pattern)
    }
}

/// A subscription that auto-decodes payloads into `T`.
pub struct TypedSubscription<T> {
    inner: Subscription,
    _marker: PhantomData<fn() -> T>,
}

impl<T> TypedSubscription<T>
where
    T: Serialize + DeserializeOwned + Send + 'static,
{
    /// Receive next event. Returns `None` if the mesh is closed.
    /// Returns `Some(Err(..))` if decoding fails (caller may choose to skip / abort).
    pub async fn recv(&mut self) -> Option<Result<(Arc<Envelope>, T)>> {
        let env = self.inner.recv().await?;
        let decoded: Result<T> =
            serde_json::from_value(env.payload.clone()).map_err(Into::into);
        Some(decoded.map(|v| (env, v)))
    }

    pub fn raw(&mut self) -> &mut Subscription {
        &mut self.inner
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
    struct OrderCancelled {
        id: u64,
        reason: String,
    }

    #[tokio::test]
    async fn typed_emit_and_subscribe() {
        let mesh = EventMesh::new();
        let set = EventSet::new(&mesh, "flow.order").unwrap();
        let created = set.kind::<OrderCreated>("created");
        let cancelled = set.kind::<OrderCancelled>("cancelled");

        let mut sub_created = set.subscribe(&created);
        let mut sub_all = set.subscribe_all();

        set.emit(&created, &OrderCreated { id: 1, amount: 9.9 })
            .await
            .unwrap();
        set.emit(
            &cancelled,
            &OrderCancelled {
                id: 1,
                reason: "user".into(),
            },
        )
        .await
        .unwrap();

        let (_env, val) = sub_created.recv().await.unwrap().unwrap();
        assert_eq!(val, OrderCreated { id: 1, amount: 9.9 });

        // sub_all should see both
        let e1 = sub_all.recv().await.unwrap();
        let e2 = sub_all.recv().await.unwrap();
        assert_eq!(e1.topic, "flow.order.created");
        assert_eq!(e2.topic, "flow.order.cancelled");
    }
}
