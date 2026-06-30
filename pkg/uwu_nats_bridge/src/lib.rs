//! NATS/JetStream bridge for the uwu_agent_engine event mesh.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────┐        NATS/JetStream       ┌──────────────────────┐
//! │   Main Agent Process │◄──────────────────────────►│  Sidecar Process      │
//! │                      │                            │                      │
//! │  NatsPublisher       │  agent.{id}.main (Core)     │  NatsSubscriber      │
//! │  (mirrors FlowHandle)│  agent.{id}.consolidation   │  (mirrors FlowRecvr) │
//! │                      │  agent.{id}.monitoring (JS) │                      │
//! │                      │  agent.{id}.system (Core)   │                      │
//! └──────────────────────┘                            └──────────────────────┘
//! ```
//!
//! # Quick start
//!
//! ```ignore
//! use uwu_nats_bridge::{
//!     NatsPublisher, NatsSubscriber, NatsSubjects, NatsConfig,
//!     PublishError, SubscribeError,
//! };
//!
//! // Publisher side (main agent process)
//! let cfg = NatsConfig::for_agent("nats://localhost:4222", "assistant", "sess-1");
//! let subjects = NatsSubjects::new("sess-1");
//! let publisher = NatsPublisher::connect(cfg, subjects).await?;
//! publisher.publish_consolidation(type_id, "consolidate.episode", &episode).await?;
//!
//! // Subscriber side (sidecar process)
//! let cfg = NatsConfig::for_sidecar("nats://localhost:4222", "consolidator");
//! let mut subscriber = NatsSubscriber::connect(cfg, "*").await?;
//! while let Some(env) = subscriber.recv_consolidation().await {
//!     let episode: Episode = env.deserialize_payload()?;
//!     // process...
//! }
//! ```

pub mod config;
pub mod publisher;
pub mod subscriber;
pub mod subjects;

pub use config::NatsConfig;
pub use publisher::{NatsPublisher, PublishError};
pub use subscriber::{NatsSubscriber, SubscribeError};
pub use subjects::NatsSubjects;
