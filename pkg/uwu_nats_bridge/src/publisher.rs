//! NATS publisher — mirrors [`uwu_event_mesh::mesh::flow_handle::FlowHandle`] API
//! but publishes across NATS/JetStream instead of local mpsc channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_nats::Client;
use async_nats::jetstream::{self, stream};
use serde::Serialize;
use uwu_event_mesh::core::serialized_envelope::SerializedEnvelope;
use uwu_event_mesh::core::type_id::{CorrelationId, TypeId};
use uwu_event_mesh::core::type_registry::TypeRegistry;
use uwu_event_mesh::mesh::flow_handle::FlowChannel;

use crate::config::NatsConfig;
use crate::subjects::NatsSubjects;

/// Publish error for the NATS bridge.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("serialize envelope: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("event mesh error: {0}")]
    Mesh(#[from] uwu_event_mesh::core::error::EventMeshError),

    #[error("NATS publish: {0}")]
    Nats(String),

    #[error("JetStream publish: {0}")]
    JetStream(String),

    #[error("NATS connect: {0}")]
    Connect(String),
}

/// Publishes typed envelopes to NATS subjects, mirroring the `FlowHandle` API.
///
/// - **Main** and **System** channels → Core NATS (fast pub, at-most-once)
/// - **Consolidation** and **Monitoring** channels → JetStream (durable, replayable)
pub struct NatsPublisher {
    client: Client,
    jetstream: jetstream::Context,
    subjects: NatsSubjects,
    seq: Arc<AtomicU64>,
    registry: Arc<TypeRegistry>,
}

impl NatsPublisher {
    /// Connect to NATS and initialize JetStream streams for durable channels.
    pub async fn connect(
        cfg: NatsConfig,
        subjects: NatsSubjects,
    ) -> Result<Self, PublishError> {
        let client = async_nats::connect_with_options(
            cfg.url,
            async_nats::ConnectOptions::new().name(cfg.connection_name),
        )
        .await
        .map_err(|e| PublishError::Connect(e.to_string()))?;

        let jetstream = jetstream::new(client.clone());

        // Ensure JetStream streams exist for consolidation + monitoring channels.
        for ch in &NatsSubjects::jetstream_channels() {
            let subject_filter = subjects.for_channel(*ch);
            let stream_name = subjects.stream_name_for(*ch);

            jetstream
                .get_or_create_stream(stream::Config {
                    name: stream_name,
                    subjects: vec![subject_filter],
                    max_age: cfg.jetstream_max_age,
                    max_bytes: cfg.jetstream_max_bytes,
                    storage: stream::StorageType::File,
                    allow_direct: true,
                    ..Default::default()
                })
                .await
                .map_err(|e| PublishError::JetStream(format!("create stream: {e}")))?;
        }

        Ok(Self {
            client,
            jetstream,
            subjects,
            seq: Arc::new(AtomicU64::new(1)),
            registry: Arc::new(TypeRegistry::new()),
        })
    }

    /// Connect with an existing TypeRegistry.
    pub async fn connect_with_registry(
        cfg: NatsConfig,
        subjects: NatsSubjects,
        registry: Arc<TypeRegistry>,
    ) -> Result<Self, PublishError> {
        let mut s = Self::connect(cfg, subjects).await?;
        s.registry = registry;
        Ok(s)
    }

    // ---- public API: mirror FlowHandle ----

    /// Publish to the **main** channel (Core NATS, at-most-once).
    pub async fn publish_main<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<(), PublishError> {
        self.publish_to(FlowChannel::Main, type_id, topic, payload).await
    }

    /// Publish to the **consolidation** channel (JetStream, durable).
    pub async fn publish_consolidation<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<(), PublishError> {
        self.publish_to(FlowChannel::Consolidation, type_id, topic, payload).await
    }

    /// Publish to the **monitoring** channel (JetStream, durable).
    pub async fn publish_monitoring<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<(), PublishError> {
        self.publish_to(FlowChannel::Monitoring, type_id, topic, payload).await
    }

    /// Publish to the **system** channel (Core NATS, at-most-once).
    pub async fn publish_system<T: Serialize>(
        &self,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<(), PublishError> {
        self.publish_to(FlowChannel::System, type_id, topic, payload).await
    }

    /// Publish a pre-built `SerializedEnvelope` directly.
    pub async fn publish_envelope(
        &self,
        channel: FlowChannel,
        mut envelope: SerializedEnvelope,
    ) -> Result<(), PublishError> {
        if envelope.sequence_number == 0 {
            envelope.sequence_number = self.seq.fetch_add(1, Ordering::SeqCst);
        }

        let subject = self.subjects.for_channel(channel);
        let payload = serde_json::to_vec(&envelope)?;

        if NatsSubjects::is_jetstream(channel) {
            self.jetstream
                .publish(subject, payload.into())
                .await
                .map_err(|e| PublishError::JetStream(format!("{e}")))?;
        } else {
            self.client
                .publish(subject, payload.into())
                .await
                .map_err(|e| PublishError::Nats(format!("{e}")))?;
        }
        Ok(())
    }

    // ---- internal helpers ----

    async fn publish_to<T: Serialize>(
        &self,
        channel: FlowChannel,
        type_id: TypeId,
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<(), PublishError> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let correlation_id: CorrelationId = self.subjects.correlation_id.clone().into();

        let envelope = SerializedEnvelope::new(
            type_id,
            topic,
            correlation_id,
            "nats-publisher",
            payload,
        )?
        .with_sequence_number(seq);

        self.publish_envelope(channel, envelope).await
    }

    /// Next sequence number (without incrementing).
    pub fn next_sequence(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    /// Events published so far.
    pub fn sequence_count(&self) -> u64 {
        self.seq.load(Ordering::Relaxed).saturating_sub(1)
    }

    /// Reference to the TypeRegistry.
    pub fn type_registry(&self) -> &Arc<TypeRegistry> {
        &self.registry
    }

    /// The NATS client (advanced use).
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Flush buffered publishes to the NATS server.
    pub async fn flush(&self) -> Result<(), PublishError> {
        self.client
            .flush()
            .await
            .map_err(|e| PublishError::Nats(format!("flush: {e}")))
    }
}

impl std::fmt::Debug for NatsPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NatsPublisher")
            .field("subjects", &self.subjects)
            .field("sequence_count", &self.sequence_count())
            .finish()
    }
}

impl Clone for NatsPublisher {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            jetstream: self.jetstream.clone(),
            subjects: self.subjects.clone(),
            seq: self.seq.clone(),
            registry: self.registry.clone(),
        }
    }
}
