//! Subtask / SubtaskDag / SubtaskStatus

use serde::{Deserialize, Serialize};

/// Subtask 状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubtaskStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed { error: String },
    Cancelled,
}

/// DAG 中的一条边（依赖关系）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskEdge {
    pub from: usize,
    pub to: usize,
}

/// 子任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: String,
    pub index: usize,
    pub description: String,
    pub status: SubtaskStatus,
    pub assigned_agent: Option<String>,
    pub max_retries: u32,
    pub retries: u32,
    pub timeout_secs: u64,
}

impl Subtask {
    pub fn new(index: usize, description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            index,
            description: description.into(),
            status: SubtaskStatus::Pending,
            assigned_agent: None,
            max_retries: 3,
            retries: 0,
            timeout_secs: 300,
        }
    }
}

/// 子任务 DAG
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubtaskDag {
    pub nodes: Vec<Subtask>,
    pub edges: Vec<SubtaskEdge>,
}

impl SubtaskDag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, subtask: Subtask) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(subtask);
        idx
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        self.edges.push(SubtaskEdge { from, to });
    }

    /// 返回所有就绪的节点（所有前置节点已完成）
    pub fn ready_nodes(&self) -> Vec<String> {
        let completed: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.status == SubtaskStatus::Completed)
            .map(|n| n.index)
            .collect();

        self.nodes
            .iter()
            .filter(|n| n.status == SubtaskStatus::Pending || n.status == SubtaskStatus::Ready)
            .filter(|n| {
                self.edges
                    .iter()
                    .filter(|e| e.to == n.index)
                    .all(|e| completed.contains(&e.from))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dag_single_node_ready() {
        let mut dag = SubtaskDag::new();
        dag.add_node(Subtask::new(0, "task1"));
        assert_eq!(dag.ready_nodes().len(), 1);
    }

    #[test]
    fn dag_dependency_blocking() {
        let mut dag = SubtaskDag::new();
        dag.add_node(Subtask::new(0, "task1"));
        dag.add_node(Subtask::new(1, "task2"));
        dag.add_edge(0, 1);
        // task2 depends on task1, neither completed → only task1 ready
        let ready = dag.ready_nodes();
        assert_eq!(ready.len(), 1);
    }
}
