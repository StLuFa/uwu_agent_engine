//! Hierarchical, dot-separated topic names with `*` (single segment) and `>` (multi-segment) wildcards.
//!
//! Example: `flow.order.created` matches `flow.*.created` and `flow.>`.

use crate::core::error::{EventMeshError, Result};

/// A concrete topic name (no wildcards).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Topic(String);

impl Topic {
    /// Validates a raw string topic without allocation.
    pub fn validate_str(s: &str) -> Result<()> {
        if s.is_empty() {
            return Err(EventMeshError::InvalidTopic("empty".into()));
        }
        for seg in s.split('.') {
            if seg.is_empty() {
                return Err(EventMeshError::InvalidTopic(s.to_string()));
            }
            if seg.contains('*') || seg.contains('>') {
                return Err(EventMeshError::InvalidTopic(format!(
                    "wildcards not allowed in concrete topic: {s}"
                )));
            }
        }
        Ok(())
    }

    pub fn new(name: impl Into<String>) -> Result<Self> {
        let s = name.into();
        Self::validate_str(&s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A subscription pattern, may contain wildcards.
///
/// - `*` matches a single segment
/// - `>` matches one-or-more trailing segments and must be the last segment
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicPattern {
    segments: Vec<String>,
}

impl TopicPattern {
    pub fn new(pattern: impl Into<String>) -> Result<Self> {
        let s = pattern.into();
        if s.is_empty() {
            return Err(EventMeshError::InvalidPattern("empty".into()));
        }
        let segs: Vec<String> = s.split('.').map(|s| s.to_string()).collect();
        for (i, seg) in segs.iter().enumerate() {
            if seg.is_empty() {
                return Err(EventMeshError::InvalidPattern(s.clone()));
            }
            if seg == ">" && i != segs.len() - 1 {
                return Err(EventMeshError::InvalidPattern(format!(
                    "`>` must be the last segment: {s}"
                )));
            }
            // segments other than `*` / `>` must not contain wildcards
            if seg != "*" && seg != ">" && (seg.contains('*') || seg.contains('>')) {
                return Err(EventMeshError::InvalidPattern(s.clone()));
            }
        }
        Ok(Self { segments: segs })
    }

    pub fn matches(&self, topic: &Topic) -> bool {
        self.matches_str(topic.as_str())
    }

    /// Optimized match over pre-split segments to avoid allocation / redundant splitting
    /// when checking many subscribers against one topic.
    pub fn matches_segments(&self, topic_segs: &[&str]) -> bool {
        let pat = &self.segments;
        let mut i = 0;
        let mut j = 0;
        while i < pat.len() {
            if pat[i] == ">" {
                return j < topic_segs.len();
            }
            if j >= topic_segs.len() {
                return false;
            }
            if pat[i] != "*" && pat[i] != topic_segs[j] {
                return false;
            }
            i += 1;
            j += 1;
        }
        j == topic_segs.len()
    }

    /// Match against a raw string topic. Cheaper than [`Self::matches`] when
    /// you only have an `&str` (e.g. directly from `Envelope::topic`).
    pub fn matches_str(&self, topic: &str) -> bool {
        // Fallback for simple tests or non-hot paths
        let segs: Vec<&str> = topic.split('.').collect();
        self.matches_segments(&segs)
    }
}

impl std::fmt::Display for TopicPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.segments.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let p = TopicPattern::new("flow.order.created").unwrap();
        assert!(p.matches(&Topic::new("flow.order.created").unwrap()));
        assert!(!p.matches(&Topic::new("flow.order.updated").unwrap()));
    }

    #[test]
    fn star_match() {
        let p = TopicPattern::new("flow.*.created").unwrap();
        assert!(p.matches(&Topic::new("flow.order.created").unwrap()));
        assert!(p.matches(&Topic::new("flow.user.created").unwrap()));
        assert!(!p.matches(&Topic::new("flow.order.updated").unwrap()));
        assert!(!p.matches(&Topic::new("flow.order.x.created").unwrap()));
    }

    #[test]
    fn gt_match() {
        let p = TopicPattern::new("flow.>").unwrap();
        assert!(p.matches(&Topic::new("flow.order").unwrap()));
        assert!(p.matches(&Topic::new("flow.order.created").unwrap()));
        assert!(!p.matches(&Topic::new("flow").unwrap()));
        assert!(!p.matches(&Topic::new("agent.order").unwrap()));
    }

    #[test]
    fn invalid() {
        assert!(Topic::new("").is_err());
        assert!(Topic::new("a..b").is_err());
        assert!(Topic::new("a.*").is_err());
        assert!(TopicPattern::new("a.>.b").is_err());
    }
}
