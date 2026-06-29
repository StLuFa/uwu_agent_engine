//! SubtaskScheduler

use crate::subtask::{SubtaskDag, SubtaskStatus};

/// 子任务调度器
pub struct SubtaskScheduler;

impl SubtaskScheduler {
    /// 获取下一个可执行的 subtask
    pub fn next_ready(dag: &SubtaskDag) -> Option<String> {
        dag.ready_nodes().into_iter().next()
    }

    /// 检查 DAG 是否全部完成
    pub fn is_complete(dag: &SubtaskDag) -> bool {
        dag.nodes.iter().all(|n| n.status == SubtaskStatus::Completed)
    }

    /// 获取进度百分比
    pub fn progress(dag: &SubtaskDag) -> f32 {
        if dag.nodes.is_empty() {
            return 1.0;
        }
        let completed = dag
            .nodes
            .iter()
            .filter(|n| n.status == SubtaskStatus::Completed)
            .count();
        completed as f32 / dag.nodes.len() as f32
    }
}
