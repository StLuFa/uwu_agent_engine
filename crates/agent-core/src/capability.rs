//! CapabilityRegistry —— 运行时动态能力注册

use crate::flow::Stage;
use std::collections::HashMap;

/// 能力处理器 trait —— 每个阶段可注册多个处理器
pub trait CapabilityHandler: Send + Sync {
    fn stage(&self) -> Stage;
    fn name(&self) -> &str;
}

/// 能力注册表 —— 运行时动态注册各阶段的能力处理器
///
/// 按 Stage 分类存储，支持同一阶段注册多个处理器（链式执行）。
pub struct CapabilityRegistry {
    handlers: HashMap<Stage, Vec<Box<dyn CapabilityHandler>>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// 注册一个能力处理器
    pub fn register(&mut self, handler: Box<dyn CapabilityHandler>) {
        self.handlers
            .entry(handler.stage())
            .or_default()
            .push(handler);
    }

    /// 获取某个阶段的所有处理器
    pub fn get(&self, stage: Stage) -> &[Box<dyn CapabilityHandler>] {
        self.handlers.get(&stage).map_or(&[], |v| v.as_slice())
    }

    /// 获取某个阶段的处理器数量
    pub fn count(&self, stage: Stage) -> usize {
        self.handlers.get(&stage).map_or(0, |v| v.len())
    }

    /// 已注册的阶段数
    pub fn stage_count(&self) -> usize {
        self.handlers.len()
    }

    /// 总处理器数
    pub fn total_count(&self) -> usize {
        self.handlers.values().map(|v| v.len()).sum()
    }

    /// 是否已注册某个阶段
    pub fn has_stage(&self, stage: Stage) -> bool {
        self.handlers.contains_key(&stage)
    }

    /// 列出所有已注册的阶段
    pub fn stages(&self) -> Vec<Stage> {
        let mut stages: Vec<_> = self.handlers.keys().copied().collect();
        stages.sort_by_key(|s| *s as u8);
        stages
    }
}

impl Default for CapabilityRegistry {
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

    struct TestHandler {
        stage: Stage,
        name: String,
    }

    impl CapabilityHandler for TestHandler {
        fn stage(&self) -> Stage {
            self.stage
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn register_and_retrieve() {
        let mut reg = CapabilityRegistry::new();
        reg.register(Box::new(TestHandler {
            stage: Stage::Perception,
            name: "text-parser".into(),
        }));
        reg.register(Box::new(TestHandler {
            stage: Stage::Reasoning,
            name: "tot-reasoner".into(),
        }));

        assert_eq!(reg.count(Stage::Perception), 1);
        assert_eq!(reg.count(Stage::Reasoning), 1);
        assert_eq!(reg.count(Stage::Execution), 0);
        assert_eq!(reg.total_count(), 2);
    }

    #[test]
    fn multiple_handlers_per_stage() {
        let mut reg = CapabilityRegistry::new();
        reg.register(Box::new(TestHandler {
            stage: Stage::Perception,
            name: "text".into(),
        }));
        reg.register(Box::new(TestHandler {
            stage: Stage::Perception,
            name: "json".into(),
        }));

        assert_eq!(reg.count(Stage::Perception), 2);
        assert_eq!(reg.get(Stage::Perception).len(), 2);
    }

    #[test]
    fn empty_registry() {
        let reg = CapabilityRegistry::new();
        assert_eq!(reg.total_count(), 0);
        assert_eq!(reg.stages().len(), 0);
    }
}
