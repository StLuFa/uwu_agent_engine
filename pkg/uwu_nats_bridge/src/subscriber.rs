//! NATS subscriber — mirrors [`uwu_event_mesh::mesh::flow_handle::FlowReceiver`] API
//! but subscribes from NATS/JetStream instead of local mpsc channels.

use std::sync::Arc;

use async_nats::Client;
use async_nats::jetstream::{self, consumer, stream};
use futures::StreamExt;
use tokio::sync::mpsc;
use uwu_event_mesh::core::serialized_envelope::SerializedEnvelope;
use uwu_event_mesh::core::type_registry::TypeRegistry;
use uwu_event_mesh::mesh::flow_handle::FlowChannel;

use crate::config::NatsConfig;
use crate::subjects::NatsSubjects;

/// Subscribe error for the NATS bridge.
#[derive(Debug, thiserror::Error)]
pub enum SubscribeError {
    #[error("NATS connect: {0}")]
    Connect(String),

    #[error("subscribe to subject {subject}: {reason}")]
    Subscribe { subject: String, reason: String },

    #[error("JetStream consumer for {channel}: {reason}")]
    Consumer { channel: String, reason: String },

    #[error("deserialize envelope: {0}")]
    Deserialize(#[from] serde_json::Error),
}

/// Buffer size for internal channels bridging NATS → consumer.
const INTERNAL_BUFFER: usize = 256;

/// Subscribes to NATS subjects and exposes per-channel receive methods,
/// mirroring the `FlowReceiver` API.
///
/// Background tasks pull from NATS/JetStream and feed internal mpsc channels.
pub struct NatsSubscriber {
    #[allow(dead_code)]
    subjects: NatsSubjects,
    #[allow(dead_code)]
    registry: Arc<TypeRegistry>,
    main_rx: mpsc::Receiver<SerializedEnvelope>,
    consolidation_rx: mpsc::Receiver<SerializedEnvelope>,
    monitoring_rx: mpsc::Receiver<SerializedEnvelope>,
    system_rx: mpsc::Receiver<SerializedEnvelope>,
    /// Keep background tasks alive.
    _tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl NatsSubscriber {
    /// Connect to NATS and start background pull tasks for each channel.
    ///
    /// `correlation_id` can be a specific session id or `"*"` for all sessions.
    pub async fn connect(
        cfg: NatsConfig,
        correlation_id: impl Into<String>,
    ) -> Result<Self, SubscribeError> {
        let cid: String = correlation_id.into();
        let subjects = NatsSubjects::new(cid.clone());

        // Clone cfg before moving fields into connect options.
        let conn_name = cfg.connection_name.clone();
        let url = cfg.url.clone();

        let client = async_nats::connect_with_options(
            url,
            async_nats::ConnectOptions::new().name(conn_name),
        )
        .await
        .map_err(|e| SubscribeError::Connect(e.to_string()))?;

        let jetstream = jetstream::new(client.clone());

        // Internal channels.
        let (main_tx, main_rx) = mpsc::channel(INTERNAL_BUFFER);
        let (consolidation_tx, consolidation_rx) = mpsc::channel(INTERNAL_BUFFER);
        let (monitoring_tx, monitoring_rx) = mpsc::channel(INTERNAL_BUFFER);
        let (system_tx, system_rx) = mpsc::channel(INTERNAL_BUFFER);

        // Subject patterns — wildcard for "*" or exact for specific session.
        let main_subject = if cid == "*" {
            "agent.*.main".to_string()
        } else {
            subjects.for_channel(FlowChannel::Main)
        };
        let sys_subject = if cid == "*" {
            "agent.*.system".to_string()
        } else {
            subjects.for_channel(FlowChannel::System)
        };
        let cons_subject = if cid == "*" {
            "agent.*.consolidation".to_string()
        } else {
            subjects.for_channel(FlowChannel::Consolidation)
        };
        let mon_subject = if cid == "*" {
            "agent.*.monitoring".to_string()
        } else {
            subjects.for_channel(FlowChannel::Monitoring)
        };

        let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        // Core NATS channels (main + system).
        tasks.push(spawn_core_sub(client.clone(), main_subject, main_tx));
        tasks.push(spawn_core_sub(client.clone(), sys_subject, system_tx));

        // JetStream channels (consolidation + monitoring).
        tasks.push(spawn_jetstream_sub(
            jetstream.clone(),
            cons_subject,
            "consolidation",
            cfg.clone(),
            consolidation_tx,
        ));
        tasks.push(spawn_jetstream_sub(
            jetstream.clone(),
            mon_subject,
            "monitoring",
            cfg,
            monitoring_tx,
        ));

        Ok(Self {
            subjects,
            registry: Arc::new(TypeRegistry::new()),
            main_rx,
            consolidation_rx,
            monitoring_rx,
            system_rx,
            _tasks: tasks,
        })
    }

    /// Connect with an existing TypeRegistry.
    pub async fn connect_with_registry(
        cfg: NatsConfig,
        correlation_id: impl Into<String>,
        registry: Arc<TypeRegistry>,
    ) -> Result<Self, SubscribeError> {
        let mut s = Self::connect(cfg, correlation_id).await?;
        s.registry = registry;
        Ok(s)
    }

    // ---- receive API: mirrors FlowReceiver ----

    pub async fn recv_main(&mut self) -> Option<SerializedEnvelope> {
        self.main_rx.recv().await
    }

    pub async fn recv_consolidation(&mut self) -> Option<SerializedEnvelope> {
        self.consolidation_rx.recv().await
    }

    pub async fn recv_monitoring(&mut self) -> Option<SerializedEnvelope> {
        self.monitoring_rx.recv().await
    }

    pub async fn recv_system(&mut self) -> Option<SerializedEnvelope> {
        self.system_rx.recv().await
    }

    /// Poll all four channels, returning the first available envelope.
    pub async fn recv_any(&mut self) -> Option<(FlowChannel, SerializedEnvelope)> {
        tokio::select! {
            env = self.main_rx.recv() => env.map(|e| (FlowChannel::Main, e)),
            env = self.consolidation_rx.recv() => env.map(|e| (FlowChannel::Consolidation, e)),
            env = self.monitoring_rx.recv() => env.map(|e| (FlowChannel::Monitoring, e)),
            env = self.system_rx.recv() => env.map(|e| (FlowChannel::System, e)),
        }
    }

    /// Split into four independent receivers.
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

    pub fn type_registry(&self) -> &Arc<TypeRegistry> {
        &self.registry
    }
}

impl std::fmt::Debug for NatsSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NatsSubscriber")
            .field("subjects", &self.subjects)
            .finish()
    }
}

// ---- background tasks ----

/// Pull from a Core NATS subject and feed into a local mpsc channel.
fn spawn_core_sub(
    client: Client,
    subject: String,
    tx: mpsc::Sender<SerializedEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sub = match client.subscribe(subject.clone()).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("NATS core subscribe failed for {subject}: {e}");
                return;
            }
        };

        while let Some(msg) = sub.next().await {
            match serde_json::from_slice::<SerializedEnvelope>(&msg.payload) {
                Ok(env) => {
                    if tx.send(env).await.is_err() {
                        break; // Receiver dropped
                    }
                }
                Err(e) => {
                    tracing::warn!("NATS core deserialize failed for {subject}: {e}");
                }
            }
        }
    })
}

/// Pull from a JetStream subject with an ephemeral consumer, feed into local mpsc.
fn spawn_jetstream_sub(
    jetstream: jetstream::Context,
    subject: String,
    channel_label: &'static str,
    cfg: NatsConfig,
    tx: mpsc::Sender<SerializedEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Retry loop — the stream may not exist yet if the publisher hasn't started.
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            match try_consume(&jetstream, &subject, channel_label, &cfg, &tx).await {
                Ok(()) => return, // Normal shutdown (receiver dropped)
                Err(e) => {
                    if attempts <= 5 {
                        tracing::info!("JetStream consumer {channel_label} not ready (attempt {attempts}/5): {e}");
                    }
                    if attempts > 30 {
                        tracing::error!("JetStream consumer {channel_label} failed after 30 attempts, giving up: {e}");
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    })
}

/// Attempt to consume from a JetStream stream. Returns Ok when the receiver is dropped.
async fn try_consume(
    jetstream: &jetstream::Context,
    subject: &str,
    channel_label: &str,
    cfg: &NatsConfig,
    tx: &mpsc::Sender<SerializedEnvelope>,
) -> Result<(), SubscribeError> {
    // Stream name: sanitize the subject pattern for filesystem safety.
    let stream_name = format!("agent_wildcard_{channel_label}");

    // Get or create the stream.
    let stream = match jetstream.get_stream(&stream_name).await {
        Ok(s) => s,
        Err(_) => {
            jetstream
                .get_or_create_stream(stream::Config {
                    name: stream_name.clone(),
                    subjects: vec![subject.to_string()],
                    max_age: cfg.jetstream_max_age,
                    max_bytes: cfg.jetstream_max_bytes,
                    storage: stream::StorageType::File,
                    allow_direct: true,
                    ..Default::default()
                })
                .await
                .map_err(|e| SubscribeError::Consumer {
                    channel: channel_label.to_string(),
                    reason: format!("create stream: {e}"),
                })?
        }
    };

    // Ephemeral pull consumer — auto-deleted on disconnect.
    let consumer = stream
        .get_or_create_consumer(
            &format!("ephemeral_{channel_label}"),
            consumer::pull::Config {
                durable_name: None,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| SubscribeError::Consumer {
            channel: channel_label.to_string(),
            reason: format!("create consumer: {e}"),
        })?;

    // Pull loop using fetch() builder API.
    loop {
        match consumer
            .fetch()
            .max_messages(32)
            .expires(std::time::Duration::from_secs(2))
            .messages()
            .await
        {
            Ok(mut batch_stream) => {
                let mut had_msg = false;
                while let Some(msg_result) = batch_stream.next().await {
                    had_msg = true;
                    match msg_result {
                        Ok(msg) => {
                            // Ack for at-most-once delivery.
                            if let Err(e) = msg.ack().await {
                                tracing::warn!("JetStream ack failed for {channel_label}: {e}");
                            }
                            match serde_json::from_slice::<SerializedEnvelope>(&msg.payload) {
                                Ok(env) => {
                                    if tx.send(env).await.is_err() {
                                        return Ok(()); // Receiver dropped → clean exit
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "JetStream deserialize failed for {channel_label}: {e}"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("JetStream batch msg error for {channel_label}: {e}");
                        }
                    }
                }
                if !had_msg {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
            Err(e) => {
                return Err(SubscribeError::Consumer {
                    channel: channel_label.to_string(),
                    reason: format!("fetch: {e}"),
                });
            }
        }
    }
}
