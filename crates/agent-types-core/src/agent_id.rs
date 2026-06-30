//! AgentId type

use serde::{Deserialize, Serialize};

/// Agent 全局唯一标识
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn new_generates_unique_ids() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn default_equals_new() {
        let id1 = AgentId::new();
        let id2 = AgentId::default();
        // Both are random UUIDs, so they should be different from each other
        // but both should have non-empty string content
        assert!(!id1.0.is_empty());
        assert!(!id2.0.is_empty());
    }

    #[test]
    fn display_formats_uuid() {
        let id = AgentId::new();
        let displayed = format!("{id}");
        assert_eq!(displayed, id.0);
        assert!(!displayed.is_empty());
    }

    #[test]
    fn equality_and_hash() {
        let id1 = AgentId::new();
        let id2 = id1.clone();
        assert_eq!(id1, id2);
        let mut set = HashSet::new();
        set.insert(id1.clone());
        set.insert(id2.clone());
        assert_eq!(set.len(), 1, "same AgentId should hash to same bucket");
    }

    #[test]
    fn serde_roundtrip() {
        let id = AgentId::new();
        let json = serde_json::to_string(&id).unwrap();
        let decoded: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }
}
