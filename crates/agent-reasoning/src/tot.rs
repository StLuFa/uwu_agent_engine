//! ToTExplorer + ToTConfig —— Tree-of-Thought beam search

use crate::sandbox::SandboxEvaluator;
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use serde::{Deserialize, Serialize};

/// ToT 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToTConfig {
    /// Beam width：每层保留的候选数
    pub beam_width: usize,
    /// 最大探索深度
    pub max_depth: usize,
    /// 候选动作生成数（每步）
    pub candidates_per_step: usize,
    /// 最低评分阈值（低于此值剪枝）
    pub min_score: f32,
}

impl Default for ToTConfig {
    fn default() -> Self {
        Self {
            beam_width: 3,
            max_depth: 4,
            candidates_per_step: 5,
            min_score: 0.3,
        }
    }
}

/// ToT 探索中的单个节点
#[derive(Debug, Clone)]
struct ToTNode {
    action: Action,
    state: AgentState,
    score: f32,
}

/// Tree-of-Thought 探索器
///
/// Beam Search：每层生成 N 个候选 → fork 推演 → 评分 → 保留 top-K → 下一层
pub struct ToTExplorer {
    config: ToTConfig,
}

impl ToTExplorer {
    pub fn new(config: ToTConfig) -> Self {
        Self { config }
    }

    /// 执行 ToT beam search
    ///
    /// `generate_candidates` 是一个函数，根据当前 state 和 depth 生成候选动作。
    /// 在真实系统中，这个函数会调用 LLM。
    pub fn explore(
        &self,
        initial_state: &AgentState,
        _goal: &str,
        generate_candidates: impl Fn(&AgentState, usize) -> Vec<Action>,
    ) -> Vec<Action> {
        // 初始化 beam：从 initial_state fork
        let initial_candidates = generate_candidates(initial_state, 0);
        let evaluated = SandboxEvaluator::evaluate_candidates(initial_state, &initial_candidates);

        let mut beam: Vec<ToTNode> = evaluated
            .into_iter()
            .map(|(action, score)| ToTNode {
                action,
                state: {
                    let mut s = initial_state.fork();
                    s.apply_hypothetical(&Action::new("root", ActionParams::new()));
                    s
                },
                score,
            })
            .filter(|n| n.score >= self.config.min_score)
            .collect();

        // Sort by score descending, keep top beam_width
        beam.sort_by(|a, b| b.score.total_cmp(&a.score));
        beam.truncate(self.config.beam_width);

        // Iterate depth
        for depth in 1..self.config.max_depth {
            let mut next_beam: Vec<ToTNode> = Vec::new();

            for node in &beam {
                let candidates = generate_candidates(&node.state, depth);
                let evaluated =
                    SandboxEvaluator::evaluate_candidates(&node.state, &candidates);

                for (action, score) in evaluated {
                    if score >= self.config.min_score {
                        let mut new_state = node.state.fork();
                        new_state.apply_hypothetical(&action);
                        next_beam.push(ToTNode {
                            action,
                            state: new_state,
                            score,
                        });
                    }
                }
            }

            // Prune to beam_width
            next_beam.sort_by(|a, b| b.score.total_cmp(&a.score));
            next_beam.truncate(self.config.beam_width);

            if next_beam.is_empty() {
                break;
            }
            beam = next_beam;
        }

        // Return actions sorted by score
        beam.sort_by(|a, b| b.score.total_cmp(&a.score));
        beam.into_iter().map(|n| n.action).collect()
    }
}

impl Default for ToTExplorer {
    fn default() -> Self {
        Self::new(ToTConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::AgentState;

    fn dummy_generator(_state: &AgentState, depth: usize) -> Vec<Action> {
        vec![
            Action::new(
                format!("action_{}_a", depth),
                ActionParams::new(),
            ),
            Action::new(
                format!("action_{}_b", depth),
                ActionParams::new(),
            ),
        ]
    }

    #[test]
    fn tot_explorer_produces_actions() {
        let state = AgentState::new();
        let explorer = ToTExplorer::default();

        let result = explorer.explore(&state, "test goal", dummy_generator);
        assert!(!result.is_empty());
        // Should not exceed beam_width
        assert!(result.len() <= 3);
    }

    #[test]
    fn tot_config_defaults() {
        let config = ToTConfig::default();
        assert_eq!(config.beam_width, 3);
        assert_eq!(config.max_depth, 4);
    }
}
