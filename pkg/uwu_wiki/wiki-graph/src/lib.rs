//! # wiki-graph
//!
//! 流程图 / 思维导图骨架。图节点自适配为 `wiki_llm::TextUnit`，不反向依赖横切层。

use serde::{Deserialize, Serialize};
use wiki_llm::TextUnit;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

/// 图节点类型（骨架）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    Process,
    Decision,
    Start,
    End,
    Idea,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Graph {
    pub id: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// 命中节点的一跳邻居（RAG context 扩展用）。
    pub fn neighbors(&self, node: &NodeId) -> Vec<&GraphNode> {
        let neighbor_ids: Vec<&NodeId> = self
            .edges
            .iter()
            .filter_map(|e| {
                if &e.from == node {
                    Some(&e.to)
                } else if &e.to == node {
                    Some(&e.from)
                } else {
                    None
                }
            })
            .collect();
        self.nodes
            .iter()
            .filter(|n| neighbor_ids.contains(&&n.id))
            .collect()
    }

    /// 节点适配为领域无关 TextUnit。
    pub fn node_to_text_unit(&self, node: &GraphNode) -> TextUnit {
        TextUnit {
            id: node.id.0.clone(),
            text: node.label.clone(),
            path: vec![self.id.clone(), node.id.0.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str) -> GraphNode {
        GraphNode {
            id: NodeId(id.into()),
            kind: NodeKind::Process,
            label: label.into(),
            embedding: None,
        }
    }

    #[test]
    fn one_hop_neighbors() {
        let mut g = Graph::new("g1");
        g.nodes.push(node("a", "A"));
        g.nodes.push(node("b", "B"));
        g.nodes.push(node("c", "C"));
        g.edges.push(Edge {
            from: NodeId("a".into()),
            to: NodeId("b".into()),
            label: None,
        });
        let ns = g.neighbors(&NodeId("a".into()));
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].id, NodeId("b".into()));
    }

    #[test]
    fn node_adapts_to_text_unit() {
        let g = Graph::new("g1");
        let n = node("a", "start here");
        let unit = g.node_to_text_unit(&n);
        assert_eq!(unit.text, "start here");
        assert_eq!(unit.id, "a");
    }
}
