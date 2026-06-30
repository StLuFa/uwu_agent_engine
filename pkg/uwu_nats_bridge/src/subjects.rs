//! NATS subject hierarchy mapping.
//!
//! Four channels map to NATS subjects:
//!
//! | Channel       | Subject pattern              | Transport   |
//! |---------------|------------------------------|-------------|
//! | Main          | `agent.{cid}.main`           | Core NATS   |
//! | Consolidation | `agent.{cid}.consolidation`  | JetStream   |
//! | Monitoring    | `agent.{cid}.monitoring`     | JetStream   |
//! | System        | `agent.{cid}.system`         | Core NATS   |

use uwu_event_mesh::mesh::flow_handle::FlowChannel;

/// The four NATS subjects derived from a correlation id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatsSubjects {
    pub correlation_id: String,
}

impl NatsSubjects {
    pub fn new(correlation_id: impl Into<String>) -> Self {
        Self {
            correlation_id: correlation_id.into(),
        }
    }

    /// Subject for a specific flow channel.
    ///
    /// ```ignore
    /// let s = NatsSubjects::new("session-001");
    /// assert_eq!(s.for_channel(FlowChannel::Main), "agent.session-001.main");
    /// ```
    pub fn for_channel(&self, channel: FlowChannel) -> String {
        let ch = channel_segment(channel);
        format!("agent.{}.{}", self.correlation_id, ch)
    }

    /// Wildcard: subscribe to all four channels for this correlation id.
    ///
    /// ```ignore
    /// assert_eq!(s.all_channels(), "agent.session-001.>");
    /// ```
    pub fn all_channels(&self) -> String {
        format!("agent.{}.>", self.correlation_id)
    }

    /// Wildcard: subscribe across ALL sessions (sidecar pattern).
    pub fn all_sessions() -> String {
        "agent.*.>".to_string()
    }

    /// Channels that should use JetStream (durable, replayable).
    pub fn jetstream_channels() -> [FlowChannel; 2] {
        [FlowChannel::Consolidation, FlowChannel::Monitoring]
    }

    /// Channels that use Core NATS (low-latency, ephemeral).
    pub fn core_channels() -> [FlowChannel; 2] {
        [FlowChannel::Main, FlowChannel::System]
    }

    /// Returns `true` if this channel should use JetStream.
    pub fn is_jetstream(channel: FlowChannel) -> bool {
        matches!(
            channel,
            FlowChannel::Consolidation | FlowChannel::Monitoring
        )
    }

    /// JetStream stream name for a channel (must be filesystem-safe).
    pub fn stream_name_for(&self, channel: FlowChannel) -> String {
        format!(
            "agent_{}_{}",
            self.correlation_id.replace(['.', '*', '>'], "_"),
            channel_segment(channel)
        )
    }

    /// JetStream consumer name (durable).
    pub fn consumer_name_for(&self, channel: FlowChannel, process: &str) -> String {
        format!("{}_{}", self.stream_name_for(channel), process)
    }
}

fn channel_segment(channel: FlowChannel) -> &'static str {
    match channel {
        FlowChannel::Main => "main",
        FlowChannel::Consolidation => "consolidation",
        FlowChannel::Monitoring => "monitoring",
        FlowChannel::System => "system",
    }
}

impl From<&str> for NatsSubjects {
    fn from(cid: &str) -> Self {
        Self::new(cid)
    }
}

impl From<String> for NatsSubjects {
    fn from(cid: String) -> Self {
        Self::new(cid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_for_main() {
        let s = NatsSubjects::new("session-001");
        assert_eq!(s.for_channel(FlowChannel::Main), "agent.session-001.main");
    }

    #[test]
    fn subject_for_consolidation() {
        let s = NatsSubjects::new("session-001");
        assert_eq!(
            s.for_channel(FlowChannel::Consolidation),
            "agent.session-001.consolidation"
        );
    }

    #[test]
    fn subject_for_monitoring() {
        let s = NatsSubjects::new("task-42");
        assert_eq!(
            s.for_channel(FlowChannel::Monitoring),
            "agent.task-42.monitoring"
        );
    }

    #[test]
    fn subject_for_system() {
        let s = NatsSubjects::new("s-1");
        assert_eq!(s.for_channel(FlowChannel::System), "agent.s-1.system");
    }

    #[test]
    fn all_channels_wildcard() {
        let s = NatsSubjects::new("session-001");
        assert_eq!(s.all_channels(), "agent.session-001.>");
    }

    #[test]
    fn all_sessions_wildcard() {
        assert_eq!(NatsSubjects::all_sessions(), "agent.*.>");
    }

    #[test]
    fn jetstream_channels_list() {
        let chs = NatsSubjects::jetstream_channels();
        assert_eq!(chs.len(), 2);
        assert!(chs.contains(&FlowChannel::Consolidation));
        assert!(chs.contains(&FlowChannel::Monitoring));
    }

    #[test]
    fn is_jetstream_check() {
        assert!(!NatsSubjects::is_jetstream(FlowChannel::Main));
        assert!(NatsSubjects::is_jetstream(FlowChannel::Consolidation));
        assert!(NatsSubjects::is_jetstream(FlowChannel::Monitoring));
        assert!(!NatsSubjects::is_jetstream(FlowChannel::System));
    }

    #[test]
    fn stream_name_sanitization() {
        let s = NatsSubjects::new("session-001");
        let name = s.stream_name_for(FlowChannel::Consolidation);
        assert_eq!(name, "agent_session-001_consolidation");
    }

    #[test]
    fn consumer_name() {
        let s = NatsSubjects::new("task-42");
        let name = s.consumer_name_for(FlowChannel::Consolidation, "consolidator");
        assert_eq!(name, "agent_task-42_consolidation_consolidator");
    }

    #[test]
    fn from_str() {
        let s: NatsSubjects = "session-001".into();
        assert_eq!(s.correlation_id, "session-001");
    }

    #[test]
    fn from_string() {
        let s: NatsSubjects = String::from("x").into();
        assert_eq!(s.correlation_id, "x");
    }
}
