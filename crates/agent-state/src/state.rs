//! AgentState struct —— Agent 世界模型核心

use crate::checkpoint::StateCheckpoint;
use crate::confidence::ConfidenceMap;
use crate::diff::StateDiff;
use crate::evaluate::StateScore;
use crate::long::LongTermWS;
use crate::mid::{ActionRecord, MidTermWS};
use crate::mvcc::StateSnapshot;
use crate::short::ShortTermWS;
use agent_types_core::{Action, ActionStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// State 全局唯一标识
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateId(pub String);

impl StateId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for StateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for StateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent 完整状态 —— 唯一真相源（Single Source of Truth）
///
/// 由三层时间尺度的工作状态 (Short/Mid/Long) 加置信度元数据组成。
/// 所有 Agent 决策基于此结构化状态，而非 scratchpad 文本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// 状态版本标识
    pub state_id: StateId,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 短程工作状态（每步更新）
    pub short_term: ShortTermWS,
    /// 中程工作状态（每 N 步更新）
    pub mid_term: MidTermWS,
    /// 长程工作状态（任务级更新）
    pub long_term: LongTermWS,
    /// 事实/假设置信度
    pub confidence: ConfidenceMap,
    /// 父状态 ID（fork 时设置）
    pub parent_state_id: Option<StateId>,
}

impl AgentState {
    // =======================================================================
    // 构造
    // =======================================================================

    /// 创建空 AgentState
    pub fn new() -> Self {
        Self {
            state_id: StateId::new(),
            timestamp: Utc::now(),
            short_term: ShortTermWS::default(),
            mid_term: MidTermWS::default(),
            long_term: LongTermWS::default(),
            confidence: ConfidenceMap::new(),
            parent_state_id: None,
        }
    }

    /// 以指定 state_id 创建
    pub fn with_id(state_id: StateId) -> Self {
        Self {
            state_id,
            ..Self::new()
        }
    }

    // =======================================================================
    // fork() —— 克隆 + 新 ID + 链接父状态
    // =======================================================================

    /// 分叉当前状态以进行沙盒推演
    ///
    /// 创建完整克隆 + 新 `StateId` + 链接 `parent_state_id`。
    /// **不修改原状态**。
    pub fn fork(&self) -> Self {
        let mut s = self.clone();
        s.state_id = StateId::new();
        s.parent_state_id = Some(self.state_id.clone());
        s
    }

    // =======================================================================
    // apply_action() —— 提交动作到主状态
    // =======================================================================

    /// 应用已提交的动作
    ///
    /// - `short_term.version += 1`
    /// - 设置 `short_term.last_action`
    /// - 向 `mid_term.action_history` 追加 Committed 记录
    pub fn apply_action(&mut self, action: &Action) {
        self.short_term.version += 1;
        self.short_term.last_action = Some(action.clone());
        self.mid_term.action_history.push(ActionRecord::new(
            action.clone(),
            ActionStatus::Committed,
        ));
    }

    // =======================================================================
    // apply_hypothetical() —— 沙盒推演（不提交）
    // =======================================================================

    /// 在分叉状态上应用假设性动作
    ///
    /// - 向 `mid_term.action_history` 追加 Hypothetical 记录
    /// - 设置 `short_term.last_action`
    /// - **不增加版本号**（沙盒状态不影响主版本）
    pub fn apply_hypothetical(&mut self, action: &Action) {
        self.mid_term.action_history.push(ActionRecord::new(
            action.clone(),
            ActionStatus::Hypothetical,
        ));
        self.short_term.last_action = Some(action.clone());
    }

    // =======================================================================
    // snapshot() —— MVCC 只读快照
    // =======================================================================

    /// 生成 MVCC 快照供 Sidecar 只读消费
    ///
    /// `snapshot_version = max(short.version, mid.version, long.version)`
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            snapshot_version: self
                .short_term
                .version
                .max(self.mid_term.version)
                .max(self.long_term.version),
            short_term: self.short_term.clone(),
            mid_term: self.mid_term.clone(),
            long_term: self.long_term.clone(),
            taken_at: Utc::now(),
        }
    }

    // =======================================================================
    // diff() —— 结构化状态差异
    // =======================================================================

    /// 与另一个 State 比较已知事实差异
    pub fn diff(&self, other: &Self) -> StateDiff {
        StateDiff::from_states(&self.mid_term.known_facts, &other.mid_term.known_facts)
    }

    // =======================================================================
    // compute_pred_error() / update_pred_error() —— JEPA 预测误差
    // =======================================================================

    /// JEPA 预测误差：当前状态预测的事实与实际事实的差异比例
    pub fn compute_pred_error(&self, actual: &Self) -> f32 {
        StateDiff::compute_pred_error(self, actual)
    }

    /// EMA 更新累积预测误差
    ///
    /// `accumulated = 0.3 × current_err + 0.7 × previous_accumulated`
    pub fn update_pred_error(&mut self, actual: &Self) {
        let err = self.compute_pred_error(actual);
        self.long_term.accumulated_pred_error =
            0.3 * err + 0.7 * self.long_term.accumulated_pred_error;
    }

    // =======================================================================
    // evaluate() —— 综合评分
    // =======================================================================

    /// 对当前状态进行综合评分
    pub fn evaluate(&self) -> StateScore {
        StateScore::evaluate(self)
    }

    // =======================================================================
    // checkpoint() / rollback() —— 持久化 + 恢复
    // =======================================================================

    /// 创建状态检查点
    pub fn checkpoint(&self) -> StateCheckpoint {
        StateCheckpoint::from_state(self)
    }

    /// 从检查点恢复
    pub fn rollback(checkpoint: &StateCheckpoint) -> Self {
        StateCheckpoint::rollback(checkpoint)
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mid::Fact;
    use agent_types_core::ActionParams;

    fn sample_state() -> AgentState {
        let mut state = AgentState::new();
        state.mid_term.known_facts.push(Fact::new("sky_color", "blue", 0.9));
        state
    }

    #[test]
    fn fork_does_not_modify_original() {
        let original = sample_state();
        let original_id = original.state_id.clone();
        let original_version = original.short_term.version;

        let forked = original.fork();

        // Original unchanged
        assert_eq!(original.state_id, original_id);
        assert_eq!(original.short_term.version, original_version);
        // Forked has new id
        assert_ne!(forked.state_id, original_id);
        // Forked parent links back
        assert_eq!(forked.parent_state_id.as_ref().unwrap(), &original_id);
        // Forked shares data (deep clone)
        assert_eq!(forked.mid_term.known_facts.len(), 1);
        assert_eq!(forked.mid_term.known_facts[0].key, "sky_color");
    }

    #[test]
    fn apply_action_increments_version() {
        let mut state = sample_state();
        let old_version = state.short_term.version;

        let action = Action::new("click", ActionParams::new().with("x", 100).with("y", 200));
        state.apply_action(&action);

        assert_eq!(state.short_term.version, old_version + 1);
        assert!(state.short_term.last_action.is_some());
        assert_eq!(state.mid_term.action_history.len(), 1);
        assert_eq!(
            state.mid_term.action_history[0].status,
            ActionStatus::Committed
        );
    }

    #[test]
    fn apply_hypothetical_does_not_increment_version() {
        let mut forked = sample_state().fork();
        let old_version = forked.short_term.version;

        let action = Action::new("test", ActionParams::new());
        forked.apply_hypothetical(&action);

        // Version NOT incremented
        assert_eq!(forked.short_term.version, old_version);
        // But action recorded as Hypothetical
        assert_eq!(forked.mid_term.action_history.len(), 1);
        assert_eq!(
            forked.mid_term.action_history[0].status,
            ActionStatus::Hypothetical
        );
    }

    #[test]
    fn snapshot_version_is_max_of_three_layers() {
        let mut state = sample_state();
        state.short_term.version = 5;
        state.mid_term.version = 3;
        state.long_term.version = 7;

        let snap = state.snapshot();
        // max(5, 3, 7) = 7
        assert_eq!(snap.snapshot_version, 7);

        state.short_term.version = 10;
        let snap2 = state.snapshot();
        // max(10, 3, 7) = 10
        assert_eq!(snap2.snapshot_version, 10);
    }

    #[test]
    fn pred_error_ema_converges() {
        let mut state = AgentState::new();
        state.mid_term.known_facts = vec![
            Fact::new("a", "1", 1.0),
            Fact::new("b", "2", 1.0),
        ];

        let mut actual = state.clone();
        actual.mid_term.known_facts = vec![
            Fact::new("a", "1", 1.0), // same
            Fact::new("b", "3", 1.0), // modified
        ];

        // 1st update: err = 1/2 = 0.5, EMA = 0.3*0.5 + 0.7*0 = 0.15
        state.update_pred_error(&actual);
        assert!((state.long_term.accumulated_pred_error - 0.15).abs() < 0.001);

        // 2nd update: EMA = 0.3*0.5 + 0.7*0.15 = 0.255
        state.update_pred_error(&actual);
        assert!((state.long_term.accumulated_pred_error - 0.255).abs() < 0.001);

        // 3rd update: EMA = 0.3*0.5 + 0.7*0.255 = 0.3285
        state.update_pred_error(&actual);
        assert!((state.long_term.accumulated_pred_error - 0.3285).abs() < 0.001);
    }

    #[test]
    fn checkpoint_rollback_roundtrip() {
        let mut state = sample_state();
        state.short_term.version = 42;
        state.mid_term.known_facts.push(Fact::new("gravity", "9.8", 0.99));

        let checkpoint = state.checkpoint();
        let restored = AgentState::rollback(&checkpoint);

        assert_eq!(restored.state_id, state.state_id);
        assert_eq!(restored.short_term.version, 42);
        assert_eq!(restored.mid_term.known_facts.len(), 2);
        assert_eq!(restored.mid_term.known_facts[1].key, "gravity");
    }
}
