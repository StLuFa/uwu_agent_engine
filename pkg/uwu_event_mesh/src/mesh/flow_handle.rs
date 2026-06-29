//! [`FlowHandle`] — the primary publish-side API for the cross-process
//! event mesh.
//!
//! # Four-channel architecture
//!
//! Every published envelope is routed to exactly one of four channels:
//!
//! | Channel        | Capacity | Consumer                              |
//! |----------------|----------|---------------------------------------|
//! | `main`         | 64       | Main agent loop (decision → action)   |
//! | `consolidation`| 256      | Sidecar consolidator (LearnNode+Guard)|
//! | `monitoring`   | 64       | Sidecar monitor (anomaly detection)   |
//! | `system`       | 128      | System events (heartbeats, config)    |
//!
//! # Usage
//!
//! ```ignore
//! let reg = Arc::new(TypeRegistry::new());
//! reg.register::<MyEvent>("domain", "event");
//!
//! let (flow, mut rx) = FlowHandle::new("task-001", reg);
//!
//! // Publish typed events — auto-assigns sequence_number.
//! flow.publish_main(TypeId::new("domain", "event"), "topic.here", &my_event)?;
//! flow.publish_consolidation(TypeId::new("domain", "other"), "topic.there", &other)?;
//!
//! // Sidecar processes its channel:
//! while let Some(env) = rx.recv_consolidation().await {
//!     // ...
//! }
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::core::error::Result;
use crate::core::serialized_envelope::SerializedEnvelope;
use crate::core::type_id::{CorrelationId, TypeId};
use crate::core::type_registry::TypeRegistry;

// ---- channel capacities per architecture §8 ----

/// Main channel capacity: 64.
pub const MAIN_CAPACITY: usize = 64;
/// Consolidation channel capacity: 256.
pub const CONSOLIDATION_CAPACITY: usize = 256;
/// Monitoring channel capacity: 64.
pub const MONITORING_CAPACITY: usize = 64;
/// System channel capacity: 128.
pub const SYSTEM_CAPACITY: usize = 128;

/// Which channel to route an envelope to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowChannel {
    /// Main agent decision-action loop.
    Main,
    /// Sidecar consolidator (LearnNode trigger + Guard egress).
    Consolidation,
    /// Sidecar monitor (anomaly detection, drift detection).
    Monitoring,
    /// System events (heartbeats, config changes, shutdown).
    System,
}

/// The publish side of the four-channel event mesh.
///
/// Cheap to clone — all clones share the same underlying channels and
/// sequence counter. Dropping the last clone closes all channels.
#[derive(Clone)]
pub struct FlowHandle {
    correlation_id: CorrelationId,
    seq: Arc<AtomicU64>,
    type_registry: Arc<TypeRegistry>,
    main_tx: mpsc::Sender<SerializedEnvelope>,
    consolidation_tx: mpsc::Sender<SerializedEnvelope>,
    monitoring_tx: mpsc::Sender<SerializedEnvelope>,
    system_tx: mpsc::Sender<SerializedEnvelope>,
    producer_id: Arc<Mutex<String>>,
}

impl FlowHandle {
    /// Create a new `FlowHandle` and the matching [`FlowReceiver`].
    ///
    /// `producer_id` identifies this process in `EventMetadata`.
    pub fn new(
        correlation_id: CorrelationId,
        type_registry: Arc<TypeRegistry>,
        producer_id: impl Into<String>,
    ) -> (Self, FlowReceiver) {
        let (main_tx, main_rx) = mpsc::channel(MAIN_CAPACITY);
        let (consolidation_tx, consolidation_rx) = mpsc::channel(CONSOLIDATION_CAPACITY);
        let (monitoring_tx, monitoring_rx) = mpsc::channel(MONITORING_CAPACITY);
        let (system_tx, system_rx) = mpsc::channel(SYSTEM_CAPACITY);

        let handle = Self {
            correlation_id,
            seq: Arc::new(AtomicU64::new(1)),
            type_registry,
            main_tx,
            consolidation_tx,
            monitoring_tx,
            system_tx,
            producer_id: Arc::new(Mutex::new(producer_id.into())),
        };

        let receiver = FlowReceiver {
            main_rx,
            consolidation_rx,
            monitoring_rx,
            system_rx,
        };

        (handle, receiver)
    }

    /// Publish a typed event to the **main** channel.
    pub async fn publish_main<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<()> {
        self.publish_to(FlowChannel::Main, type_id, topic, payload)
            .await
    }

    /// Publish a typed event to the **consolidation** channel.
    pub async fn publish_consolidation<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<()> {
        self.publish_to(FlowChannel::Consolidation, type_id, topic, payload)
            .await
    }

    /// Publish a typed event to the **monitoring** channel.
    pub async fn publish_monitoring<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<()> {
        self.publish_to(FlowChannel::Monitoring, type_id, topic, payload)
            .await
    }

    /// Publish a typed event to the **system** channel.
    pub async fn publish_system<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<()> {
        self.publish_to(FlowChannel::System, type_id, topic, payload)
            .await
    }

    /// Publish a typed event to a specific channel.
    ///
    /// Auto-assigns a monotonic `sequence_number` from this handle's counter.
    pub async fn publish_to<T: Serialize>(
        &self,
        channel: FlowChannel,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<()> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let producer = self.producer_id.lock().clone();
        let envelope = SerializedEnvelope::new(
            type_id,
            topic,
            self.correlation_id.clone(),
            producer,
            payload,
        )?
        .with_sequence_number(seq);

        // If this handle is being used for replay, propagate the replay marker.
        // (Set externally via `set_replay_id`.)

        match channel {
            FlowChannel::Main => {
                self.main_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::Consolidation => {
                self.consolidation_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::Monitoring => {
                self.monitoring_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::System => {
                self.system_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
        }

        Ok(())
    }

    /// Publish a pre-built [`SerializedEnvelope`] to a specific channel.
    ///
    /// The envelope's `sequence_number` is overwritten with the next value
    /// from this handle's counter unless it already has a non-zero value.
    pub async fn publish_envelope(
        &self,
        channel: FlowChannel,
        mut envelope: SerializedEnvelope,
    ) -> Result<()> {
        if envelope.sequence_number == 0 {
            envelope.sequence_number = self.seq.fetch_add(1, Ordering::SeqCst);
        }
        match channel {
            FlowChannel::Main => {
                self.main_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::Consolidation => {
                self.consolidation_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::Monitoring => {
                self.monitoring_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
            FlowChannel::System => {
                self.system_tx.send(envelope).await.map_err(|_| {
                    crate::core::error::EventMeshError::Closed
                })?;
            }
        }
        Ok(())
    }

    /// Correlation id shared by all envelopes from this handle.
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    /// Next sequence number that will be assigned (without incrementing).
    pub fn next_sequence(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    /// Number of events published so far through this handle.
    pub fn sequence_count(&self) -> u64 {
        self.seq.load(Ordering::Relaxed).saturating_sub(1)
    }

    /// Reference to the type registry (for boundary checks).
    pub fn type_registry(&self) -> &Arc<TypeRegistry> {
        &self.type_registry
    }

    /// Update the producer id (e.g. after agent name change).
    pub fn set_producer_id(&self, id: impl Into<String>) {
        *self.producer_id.lock() = id.into();
    }
}

impl std::fmt::Debug for FlowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowHandle")
            .field("correlation_id", &self.correlation_id)
            .field("sequence_count", &self.sequence_count())
            .finish()
    }
}

/// The consumer side of a [`FlowHandle`]'s four channels.
///
/// Each `recv_*` method returns `None` when the corresponding sender is
/// dropped (i.e., all [`FlowHandle`] clones have been dropped).
pub struct FlowReceiver {
    main_rx: mpsc::Receiver<SerializedEnvelope>,
    consolidation_rx: mpsc::Receiver<SerializedEnvelope>,
    monitoring_rx: mpsc::Receiver<SerializedEnvelope>,
    system_rx: mpsc::Receiver<SerializedEnvelope>,
}

impl FlowReceiver {
    /// Receive from the main channel.
    pub async fn recv_main(&mut self) -> Option<SerializedEnvelope> {
        self.main_rx.recv().await
    }

    /// Receive from the consolidation channel.
    pub async fn recv_consolidation(&mut self) -> Option<SerializedEnvelope> {
        self.consolidation_rx.recv().await
    }

    /// Receive from the monitoring channel.
    pub async fn recv_monitoring(&mut self) -> Option<SerializedEnvelope> {
        self.monitoring_rx.recv().await
    }

    /// Receive from the system channel.
    pub async fn recv_system(&mut self) -> Option<SerializedEnvelope> {
        self.system_rx.recv().await
    }

    /// Try-receive from the main channel without blocking.
    pub fn try_recv_main(
        &mut self,
    ) -> std::result::Result<SerializedEnvelope, mpsc::error::TryRecvError> {
        self.main_rx.try_recv()
    }

    /// Try-receive from the consolidation channel without blocking.
    pub fn try_recv_consolidation(
        &mut self,
    ) -> std::result::Result<SerializedEnvelope, mpsc::error::TryRecvError> {
        self.consolidation_rx.try_recv()
    }

    /// Try-receive from the monitoring channel without blocking.
    pub fn try_recv_monitoring(
        &mut self,
    ) -> std::result::Result<SerializedEnvelope, mpsc::error::TryRecvError> {
        self.monitoring_rx.try_recv()
    }

    /// Try-receive from the system channel without blocking.
    pub fn try_recv_system(
        &mut self,
    ) -> std::result::Result<SerializedEnvelope, mpsc::error::TryRecvError> {
        self.system_rx.try_recv()
    }

    /// Poll all four channels, returning the first available envelope.
    ///
    /// Useful for dispatch loops that handle all channels in one task.
    pub async fn recv_any(&mut self) -> Option<(FlowChannel, SerializedEnvelope)> {
        tokio::select! {
            env = self.main_rx.recv() => env.map(|e| (FlowChannel::Main, e)),
            env = self.consolidation_rx.recv() => env.map(|e| (FlowChannel::Consolidation, e)),
            env = self.monitoring_rx.recv() => env.map(|e| (FlowChannel::Monitoring, e)),
            env = self.system_rx.recv() => env.map(|e| (FlowChannel::System, e)),
        }
    }

    /// Split the receiver into its four constituent channels so each
    /// can be driven by a separate task.
    pub fn into_parts(
        self,
    ) -> (
        mpsc::Receiver<SerializedEnvelope>,
        mpsc::Receiver<SerializedEnvelope>,
        mpsc::Receiver<SerializedEnvelope>,
        mpsc::Receiver<SerializedEnvelope>,
    ) {
        (
            self.main_rx,
            self.consolidation_rx,
            self.monitoring_rx,
            self.system_rx,
        )
    }
}

impl std::fmt::Debug for FlowReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowReceiver").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestEvent {
        msg: String,
    }

    fn setup() -> (FlowHandle, FlowReceiver, Arc<TypeRegistry>) {
        let reg = Arc::new(TypeRegistry::new());
        reg.register::<TestEvent>("test", "event");
        let (handle, receiver) = FlowHandle::new("test-flow-1".into(), reg.clone(), "test-producer");
        (handle, receiver, reg)
    }

    #[tokio::test]
    async fn publish_and_receive_main() {
        let (handle, mut rx, _reg) = setup();
        let tid = TypeId::new("test", "event");
        let payload = TestEvent { msg: "hello main".into() };

        handle
            .publish_main(tid.clone(), "test.topic", &payload)
            .await
            .unwrap();

        let env = rx.recv_main().await.unwrap();
        assert_eq!(env.type_id, tid);
        assert_eq!(env.sequence_number, 1);
        assert_eq!(env.correlation_id, "test-flow-1");

        let decoded: TestEvent = env.deserialize_payload().unwrap();
        assert_eq!(decoded, payload);
    }

    #[tokio::test]
    async fn four_channels_independent() {
        let (handle, mut rx, _reg) = setup();
        let tid = TypeId::new("test", "event");
        let p = TestEvent { msg: "x".into() };

        handle.publish_main(tid.clone(), "a", &p).await.unwrap();
        handle.publish_consolidation(tid.clone(), "b", &p).await.unwrap();
        handle.publish_monitoring(tid.clone(), "c", &p).await.unwrap();
        handle.publish_system(tid.clone(), "d", &p).await.unwrap();

        assert!(rx.recv_main().await.is_some());
        assert!(rx.recv_consolidation().await.is_some());
        assert!(rx.recv_monitoring().await.is_some());
        assert!(rx.recv_system().await.is_some());
    }

    #[tokio::test]
    async fn sequence_numbers_monotonic() {
        let (handle, mut rx, _reg) = setup();
        let tid = TypeId::new("test", "event");
        let p = TestEvent { msg: "seq".into() };

        for i in 1..=5u64 {
            handle.publish_main(tid.clone(), "t", &p).await.unwrap();
            let env = rx.recv_main().await.unwrap();
            assert_eq!(env.sequence_number, i);
        }
    }

    #[tokio::test]
    async fn recv_any_multiplexes() {
        let (handle, mut rx, _reg) = setup();
        let tid = TypeId::new("test", "event");
        let p = TestEvent { msg: "any".into() };

        handle.publish_system(tid.clone(), "sys", &p).await.unwrap();
        handle.publish_main(tid.clone(), "main", &p).await.unwrap();

        let mut seen_main = false;
        let mut seen_sys = false;
        for _ in 0..2 {
            match rx.recv_any().await {
                Some((FlowChannel::Main, _)) => seen_main = true,
                Some((FlowChannel::System, _)) => seen_sys = true,
                other => panic!("unexpected: {other:?}"),
            }
        }
        assert!(seen_main && seen_sys);
    }

    #[tokio::test]
    async fn clone_handle_shares_channels() {
        let (handle, mut rx, _reg) = setup();
        let h2 = handle.clone();
        let tid = TypeId::new("test", "event");
        let p = TestEvent { msg: "clone".into() };

        handle.publish_main(tid.clone(), "a", &p).await.unwrap();
        h2.publish_main(tid.clone(), "b", &p).await.unwrap();

        let e1 = rx.recv_main().await.unwrap();
        let e2 = rx.recv_main().await.unwrap();
        // Sequence numbers are shared — should be 1 and 2.
        assert_eq!(e1.sequence_number + e2.sequence_number, 3); // 1+2
    }
}
