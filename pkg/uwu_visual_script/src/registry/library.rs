//! 节点定义与注册表。

use crate::error::{VsError, VsResult};
use crate::model::{Node, Pin};
use crate::registry::runner::RunnerKind;
use dashmap::DashMap;
use std::sync::Arc;

/// 节点纯度：决定调度策略与可优化空间。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Purity {
    /// 无副作用：可缓存、可折叠、可池化。
    Pure,
    /// 有副作用：必须沿 exec 流推进。
    Impure,
}

/// exec 流分发指令；节点执行后告诉 VM 走哪条 exec 出针。
#[derive(Clone, Debug)]
pub enum ExecNext {
    /// 走指定名字的 exec 输出针。
    Pin(String),
    /// 没有后续 exec（终止此分支）。
    End,
}

#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct NodeDefinition {
    pub id: String,
    pub purity: Purity,
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    pub runner: RunnerKind,
}

impl NodeDefinition {
    /// 是否是事件入口节点（无 exec 输入，至少一个 exec 输出，且为 Impure）。
    pub fn is_event(&self) -> bool {
        let has_exec_in = self.inputs.iter().any(|p| p.is_exec());
        let has_exec_out = self.outputs.iter().any(|p| p.is_exec());
        !has_exec_in && has_exec_out && self.purity == Purity::Impure
    }

    /// data 输入针下标列表（exec 不计入）。
    pub fn data_inputs(&self) -> Vec<usize> {
        self.inputs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_exec())
            .map(|(i, _)| i)
            .collect()
    }

    /// data 输出针下标列表。
    pub fn data_outputs(&self) -> Vec<usize> {
        self.outputs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_exec())
            .map(|(i, _)| i)
            .collect()
    }
}

/// 抽象节点注册表。
///
/// 编译器与 instantiate 期都通过该 trait 访问节点定义。把"节点解析"行为
/// 与具体实现解耦，便于上层 crate（`nono_skill::SkillNodeRegistry`）注入
/// 自己的节点来源（manifest、远程 ability 索引、热更新）而不必复制
/// `NodeLibrary` 的内存模型。
pub trait NodeRegistry: Send + Sync {
    fn get(&self, id: &str) -> Option<Arc<NodeDefinition>>;

    fn resolve(&self, node: &Node) -> VsResult<Arc<NodeDefinition>> {
        self.get(&node.def.id)
            .ok_or_else(|| VsError::UnknownDef(node.def.id.clone()))
    }

    /// 稳定 manifest，用于派生 `RegistryVersion`。
    ///
    /// 默认实现：列出全部 def_id 并排序后用 `\n` 拼接。子类型若有更精细
    /// 的 schema 摘要（如版本号、签名）应当覆盖该方法。
    fn manifest(&self) -> Vec<String> {
        Vec::new()
    }
}

/// `DashMap`-backed shared registry. Lock-free concurrent reads make this a
/// drop-in replacement for `NodeLibrary` in long-running runtimes that
/// register node defs from multiple threads (skill-publish, hot-reload).
#[derive(Default)]
pub struct ConcurrentNodeLibrary {
    defs: DashMap<String, Arc<NodeDefinition>>,
}

impl ConcurrentNodeLibrary {
    pub fn new() -> Arc<Self> { Arc::new(Self::default()) }

    pub fn register(&self, def: NodeDefinition) {
        self.defs.insert(def.id.clone(), Arc::new(def));
    }

    pub fn len(&self) -> usize { self.defs.len() }
}

impl NodeRegistry for ConcurrentNodeLibrary {
    fn get(&self, id: &str) -> Option<Arc<NodeDefinition>> {
        self.defs.get(id).map(|v| v.clone())
    }

    fn manifest(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.defs.iter().map(|e| e.key().clone()).collect();
        ids.sort();
        ids
    }
}

#[derive(Default, Clone)]
pub struct NodeLibrary {
    defs: std::collections::HashMap<String, Arc<NodeDefinition>>,
}

impl NodeLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, def: NodeDefinition) {
        self.defs.insert(def.id.clone(), Arc::new(def));
    }

    pub fn get(&self, id: &str) -> Option<Arc<NodeDefinition>> {
        self.defs.get(id).cloned()
    }

    pub fn resolve(&self, node: &Node) -> VsResult<Arc<NodeDefinition>> {
        self.get(&node.def.id)
            .ok_or_else(|| VsError::UnknownDef(node.def.id.clone()))
    }

    /// 注册全部内置节点，详见 [`crate::builtin`]。
    pub fn with_builtins() -> Self {
        let mut lib = Self::new();
        crate::builtin::register_all(&mut lib);
        lib
    }
}

impl NodeRegistry for NodeLibrary {
    fn get(&self, id: &str) -> Option<Arc<NodeDefinition>> {
        self.defs.get(id).cloned()
    }

    fn manifest(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.defs.keys().cloned().collect();
        ids.sort();
        ids
    }
}
