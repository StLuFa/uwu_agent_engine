//! LongTermWS + TaskProgress + BudgetConsumed —— 任务级长程工作状态

use chrono::Duration;
use serde::{Deserialize, Serialize};

/// 任务进度追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// 目标描述
    pub goal_description: String,
    /// 已完成的子任务数
    pub subtasks_completed: u32,
    /// 子任务总数（None 表示未知）
    pub subtasks_total: Option<u32>,
    /// 当前阶段的人类可读描述
    pub current_phase: String,
    /// 预估完成比例 [0.0, 1.0]
    pub estimated_completion: f32,
}

impl TaskProgress {
    pub fn new(goal_description: impl Into<String>) -> Self {
        Self {
            goal_description: goal_description.into(),
            subtasks_completed: 0,
            subtasks_total: None,
            current_phase: "initializing".into(),
            estimated_completion: 0.0,
        }
    }

    /// 归一化完成比例
    /// - 若已知子任务总数：completed / total
    /// - 否则使用 estimated_completion
    pub fn fraction(&self) -> f32 {
        match self.subtasks_total {
            Some(total) if total > 0 => {
                (self.subtasks_completed as f32 / total as f32).clamp(0.0, 1.0)
            }
            _ => self.estimated_completion.clamp(0.0, 1.0),
        }
    }

    /// 是否已完成
    pub fn is_complete(&self) -> bool {
        self.fraction() >= 1.0
    }
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self::new("")
    }
}

/// 预算消耗追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConsumed {
    /// 已使用 token 数
    pub tokens_used: u64,
    /// 已用时间
    pub elapsed: Duration,
    /// 已重试次数
    pub retries: u32,
}

impl BudgetConsumed {
    pub fn new() -> Self {
        Self {
            tokens_used: 0,
            elapsed: Duration::zero(),
            retries: 0,
        }
    }

    /// 计算剩余预算比例 —— 取 token/时间/重试三者最紧张的维度
    ///
    /// `cost_remaining = min(1 - tokens/max_tokens, 1 - time/max_time, 1 - retries/max_retries)`
    ///
    /// 被 Metacognition 用于计算 TTS（Time To Stop）信号。
    pub fn cost_remaining_fraction(
        &self,
        max_tokens: u64,
        max_time: Duration,
        max_retries: u32,
    ) -> f32 {
        let token_remaining = if max_tokens > 0 {
            1.0 - (self.tokens_used as f32 / max_tokens as f32)
        } else {
            1.0
        };

        let time_remaining = if max_time > Duration::zero() {
            1.0 - (self.elapsed.num_milliseconds() as f32
                / max_time.num_milliseconds().max(1) as f32)
        } else {
            1.0
        };

        let retry_remaining = if max_retries > 0 {
            1.0 - (self.retries as f32 / max_retries as f32)
        } else {
            1.0
        };

        token_remaining
            .min(time_remaining)
            .min(retry_remaining)
            .clamp(0.0, 1.0)
    }
}

impl Default for BudgetConsumed {
    fn default() -> Self {
        Self::new()
    }
}

/// 长程工作状态 —— 任务级更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermWS {
    /// MVCC 版本号，任务级 +1
    pub version: u64,
    /// 任务进度
    pub task_progress: TaskProgress,
    /// 累积预测误差（EMA 平滑）
    pub accumulated_pred_error: f32,
    /// 预算消耗
    pub budget_consumed: BudgetConsumed,
}

impl Default for LongTermWS {
    fn default() -> Self {
        Self {
            version: 0,
            task_progress: TaskProgress::default(),
            accumulated_pred_error: 0.0,
            budget_consumed: BudgetConsumed::default(),
        }
    }
}
