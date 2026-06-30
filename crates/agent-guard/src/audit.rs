//! AuditLog

use crate::GuardViolation;
use agent_types_core::Action;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 审计事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub action_command: String,
    pub violations: Vec<GuardViolation>,
    pub blocked: bool,
}

/// 审计日志 —— 记录所有 Guard 事件
pub struct AuditLog {
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditLog {
    pub fn new(_path: Option<&str>) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// 记录一次 Guard 命中
    pub async fn log_guard_hit(&self, action: &Action, violations: &[GuardViolation]) {
        let mut events = self.events.lock().unwrap();
        events.push(AuditEvent {
            timestamp: Utc::now(),
            action_command: action.command.clone(),
            violations: violations.to_vec(),
            blocked: true,
        });
    }

    /// 记录一次通过
    pub async fn log_pass(&self, action: &Action) {
        let mut events = self.events.lock().unwrap();
        events.push(AuditEvent {
            timestamp: Utc::now(),
            action_command: action.command.clone(),
            violations: vec![],
            blocked: false,
        });
    }

    /// 总事件数
    pub fn total_events(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// 被阻断的事件数
    pub fn blocked_count(&self) -> usize {
        self.events.lock().unwrap().iter().filter(|e| e.blocked).count()
    }

    /// 最近 N 条事件
    pub fn recent(&self, n: usize) -> Vec<AuditEvent> {
        let events = self.events.lock().unwrap();
        events.iter().rev().take(n).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuardViolation, ViolationLevel};
    use agent_types_core::ActionParams;

    #[tokio::test]
    async fn audit_log_records_blocked() {
        let log = AuditLog::new(None);
        let action = Action::new("rm_rf", ActionParams::new());
        let violations = vec![GuardViolation {
            rule: "no-rm-rf".into(),
            level: ViolationLevel::Critical,
            message: "blocked".into(),
        }];
        log.log_guard_hit(&action, &violations).await;
        assert_eq!(log.total_events(), 1);
        assert_eq!(log.blocked_count(), 1);
    }

    #[tokio::test]
    async fn audit_log_records_pass() {
        let log = AuditLog::new(None);
        let action = Action::new("click", ActionParams::new());
        log.log_pass(&action).await;
        assert_eq!(log.total_events(), 1);
        assert_eq!(log.blocked_count(), 0, "pass events should not count as blocked");
    }

    #[tokio::test]
    async fn audit_log_mixed_events() {
        let log = AuditLog::new(None);
        let safe = Action::new("click", ActionParams::new());
        let dangerous = Action::new("rm_rf", ActionParams::new());
        let violations = vec![GuardViolation {
            rule: "no-rm-rf".into(),
            level: ViolationLevel::Critical,
            message: "blocked".into(),
        }];

        log.log_pass(&safe).await;
        log.log_guard_hit(&dangerous, &violations).await;
        log.log_pass(&safe).await;

        assert_eq!(log.total_events(), 3);
        assert_eq!(log.blocked_count(), 1);
    }

    #[tokio::test]
    async fn audit_log_recent_returns_latest_first() {
        let log = AuditLog::new(None);
        for i in 0..5u32 {
            let action = Action::new(
                format!("op_{i}"),
                ActionParams::new(),
            );
            log.log_pass(&action).await;
        }
        let recent = log.recent(3);
        assert_eq!(recent.len(), 3);
        // Most recent first (reverse chronological)
        assert_eq!(recent[0].action_command, "op_4");
        assert_eq!(recent[1].action_command, "op_3");
        assert_eq!(recent[2].action_command, "op_2");
    }
}
