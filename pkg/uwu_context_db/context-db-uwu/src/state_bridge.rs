//! State 桥接（M3 骨架）：State 快照读写 + fork 推演沙盒。
//!
//! 通过注入的 core 窄端口访问存储；fork/晋升委托 version 层。

use crate::PredErrorSample;
use agent_context_db_core::{ContentRepo, FsOps, Result, StateScope};
use std::sync::Arc;

/// State 快照的最小占位（真实类型来自 agent-state，M3 对接时替换）。
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub agent_id: String,
    pub scope: StateScope,
    /// 派生标量：从事实层重算的 EMA 投影（见 §6.3 真值源边界）。
    pub accumulated_pred_error: f32,
    pub payload: serde_json::Value,
}

/// fork 句柄：推演期间写入 fork 分支，结束后晋升或回滚。
#[derive(Debug, Clone)]
pub struct ForkHandle {
    pub agent_id: String,
    pub scope: StateScope,
    pub branch: String,
}

pub struct StateBridge<S: FsOps + ContentRepo> {
    store: Arc<S>,
}

impl<S: FsOps + ContentRepo> StateBridge<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// 从 FS 加载 State 快照（骨架：读 L1/L2 反序列化）。
    pub async fn load(&self, _agent_id: &str, _scope: StateScope) -> Result<StateSnapshot> {
        // 1. 读 .overview.md (L1) 判版本 → 2. 读 state.json (L2) 反序列化
        unimplemented!("M3 对接 agent-state 时实现")
    }

    /// fork 推演沙盒（骨架：复制当前 State 到 _forks/{id}/）。
    pub async fn fork(&self, agent_id: &str, scope: StateScope) -> Result<ForkHandle> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        Ok(ForkHandle {
            agent_id: agent_id.to_string(),
            scope,
            branch: format!("fork-state-{}", id),
        })
    }

    /// 比较 fork 与基线的预测误差（读派生标量）。
    pub async fn compare_fork_pred_error(
        &self,
        _fork: &ForkHandle,
        _baseline: &ForkHandle,
    ) -> Result<Vec<PredErrorSample>> {
        unimplemented!("M3 对接 agent-metacognition 时实现")
    }
}
