//! Episode consolidation

use crate::types::{Memory, MemoryType};
use crate::embedding::Embedding;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一段经历 —— 一次完整的交互回合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Episode ID
    pub episode_id: String,
    /// Agent ID
    pub agent_id: String,
    /// 任务目标
    pub goal: String,
    /// 执行的动作序列
    pub actions: Vec<String>,
    /// 观察结果
    pub observations: Vec<String>,
    /// 最终结果
    pub outcome: String,
    /// 成功与否
    pub success: bool,
    /// 学到的经验（提取后填入）
    pub extracted_insights: Vec<String>,
    /// 发生时间
    pub occurred_at: DateTime<Utc>,
}

impl Episode {
    pub fn new(
        agent_id: impl Into<String>,
        goal: impl Into<String>,
        outcome: impl Into<String>,
        success: bool,
    ) -> Self {
        Self {
            episode_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            goal: goal.into(),
            actions: Vec::new(),
            observations: Vec::new(),
            outcome: outcome.into(),
            success,
            extracted_insights: Vec::new(),
            occurred_at: Utc::now(),
        }
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.actions.push(action.into());
        self
    }

    pub fn with_observation(mut self, obs: impl Into<String>) -> Self {
        self.observations.push(obs.into());
        self
    }
}

/// 将 Episode 巩固为一组 Memory 记录
///
/// 提取策略：
/// - 情景记忆：结果 + 全部动作列表
/// - 语义记忆：提取的洞察（success → 存入成功的模式，failure → 存入失败原因）
/// - 程序记忆：成功的动作序列（作为可复用的流程）
pub fn consolidate_episode(episode: &Episode, embedding_dim: usize) -> Vec<Memory> {
    let mut memories = Vec::new();

    // 1. 情景记忆 —— 记录整个事件
    let episodic_content = format!(
        "Goal: {}\nActions: {}\nObservations: {}\nOutcome: {}",
        episode.goal,
        episode.actions.join(" → "),
        episode.observations.join(" | "),
        episode.outcome
    );
    let mut em = Memory::new(
        MemoryType::Episodic,
        &episodic_content,
        Embedding::mock(&episodic_content, embedding_dim).values,
    )
    .with_agent(&episode.agent_id);
    if episode.success {
        em.score = crate::types::MemoryScore::new(0.8, 1.0, 0.5);
    } else {
        em.score = crate::types::MemoryScore::new(0.2, 1.0, 0.5);
    }
    memories.push(em);

    // 2. 语义记忆 —— 提取洞察
    for insight in &episode.extracted_insights {
        let sm = Memory::new(
            MemoryType::Semantic,
            insight,
            Embedding::mock(insight, embedding_dim).values,
        )
        .with_agent(&episode.agent_id);
        memories.push(sm);
    }

    // 3. 程序记忆 —— 成功的动作序列作为可复用流程
    if episode.success && !episode.actions.is_empty() {
        let procedure = format!(
            "To {}: {}",
            episode.goal,
            episode.actions.join(" → ")
        );
        memories.push(
            Memory::new(
                MemoryType::Procedural,
                &procedure,
                Embedding::mock(&procedure, embedding_dim).values,
            )
            .with_agent(&episode.agent_id),
        );
    }

    memories
}
