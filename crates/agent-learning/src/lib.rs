//! # agent-learning
//!
//! 学习触发 —— LearnNode 条件触发 + SkillTarget 本地/远程 + Guard egress 博弈。
//!
//! Episode 完成后触发学习评估，根据条件决定是否提取 Skill。
//!
//! ## 自进化失控防护
//!
//! 1. 版本化：每次 ExtractSkill 生成 SkillVersion（hash + timestamp + episode_id）
//! 2. 沙箱验证：新 Skill 先在 fork() 的 State 沙盒中运行
//! 3. 回滚机制：GuardLayer 检测异常 → 自动回滚至上一 SkillVersion
//! 4. 人工审批：McpRemote 写入必须 explicit_user_approval = true
//! 5. 配置开关：mcp_skill_write_enabled = false（默认关闭）

pub mod conditions;
mod skill;
mod trigger;

pub use skill::{SkillTarget, SkillVersion};
pub use trigger::{LearnCondition, LearnDecision, LearnTrigger};

use agent_state::AgentState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Episode —— 一次完整的任务执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub episode_id: String,
    pub session_id: String,
    pub task_id: Option<String>,
    pub state_before: Option<AgentState>,
    pub state_after: Option<AgentState>,
    pub actions_taken: Vec<String>,
    pub outcome: EpisodeOutcome,
    pub timestamp: DateTime<Utc>,
}

/// Episode 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EpisodeOutcome {
    Success { confidence: f32 },
    PartialSuccess { succeeded: Vec<String>, failed: Vec<String> },
    Failure { error: String },
}
