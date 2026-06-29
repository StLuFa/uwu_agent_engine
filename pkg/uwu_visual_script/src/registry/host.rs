//! 宿主服务：日志 / 变量 / 事件，以及给 runner 用的 [`InvokeCtx`]。

use crate::error::VsResult;
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// 节点流式输出片段。
///
/// - `Delta`：节点向外推送的增量数据（如 LLM token、子任务进度对象）。
/// - `Progress`：人可读的进度信号，`ratio ∈ [0,1]`。
/// - `Final`：节点最终值。VM 不强制要求节点发 `Final`；普通节点直接通过
///   `outputs` 写回即可。
///
/// `Serialize + Deserialize` so it can ride the `ExecutionEvent` stream over
/// process boundaries (RPC executor, trajectory replay, cache warmup).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Chunk {
    Delta(Value),
    Progress { ratio: f32, message: Option<String> },
    Final(Value),
}

/// VM/节点共享的 chunk 发送端。`None` 表示当前不在流式模式。
pub type ChunkTx = mpsc::Sender<Chunk>;

/// 宿主能力接口。后续可由 `nono_memory` 或 `nono_agent` 提供实现。
pub trait HostServices: Send + Sync {
    fn log(&mut self, level: LogLevel, msg: &str);
    fn var_get(&self, name: &str) -> Option<Value>;
    fn var_set(&mut self, name: &str, value: Value);
}

/// 权限能力抽象。`uwu_visual_script` 只传递通用 action/scope，不绑定任何业务权限模型。
pub trait PermissionGate: Send + Sync {
    fn check_permission(&self, action: &str, scope: &str) -> VsResult<()>;
}

/// 预算能力抽象。维度名由宿主约定，例如 `steps`、`tokens`、`money_usd`。
pub trait BudgetMeter: Send + Sync {
    fn consume_budget(&self, dimension: &str, amount: f64) -> VsResult<()>;
}

/// Trace 能力抽象。宿主可把事件映射到 OpenTelemetry、日志或 nono_ability trace。
pub trait TraceSink: Send + Sync {
    fn record_trace(&self, event: &str, attrs: &[(&str, Value)]);
}

/// 节点调用阶段。
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NodePhase {
    Before,
    After,
}

/// Middleware 看到的节点调用信息。
pub struct NodeCallInfo<'a> {
    pub node_id: u32,
    pub def_id: &'a str,
    pub phase: NodePhase,
}

/// 节点级 middleware。适合统一做权限、预算、trace、审计等横切逻辑。
pub trait NodeMiddleware: Send + Sync {
    fn on_node(&self, info: NodeCallInfo<'_>, ctx: &mut InvokeCtx<'_>) -> VsResult<()>;
}

/// VM 单次运行的外部环境。所有字段可选，保持引擎可独立使用。
#[derive(Default)]
pub struct ExecutionEnv<'a> {
    pub cancel: Option<&'a CancellationToken>,
    pub chunk_tx: Option<&'a ChunkTx>,
    pub permissions: Option<&'a dyn PermissionGate>,
    pub budget: Option<&'a dyn BudgetMeter>,
    pub trace: Option<&'a dyn TraceSink>,
    pub external: Option<&'a (dyn Any + Send + Sync)>,
    pub middleware: Vec<&'a dyn NodeMiddleware>,
}

impl<'a> ExecutionEnv<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cancel(mut self, cancel: &'a CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_chunk_tx(mut self, chunk_tx: &'a ChunkTx) -> Self {
        self.chunk_tx = Some(chunk_tx);
        self
    }

    pub fn with_permissions(mut self, permissions: &'a dyn PermissionGate) -> Self {
        self.permissions = Some(permissions);
        self
    }

    pub fn with_budget(mut self, budget: &'a dyn BudgetMeter) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn with_trace(mut self, trace: &'a dyn TraceSink) -> Self {
        self.trace = Some(trace);
        self
    }

    pub fn with_external<T: Any + Send + Sync>(mut self, external: &'a T) -> Self {
        self.external = Some(external);
        self
    }

    pub fn with_middleware(mut self, middleware: &'a dyn NodeMiddleware) -> Self {
        self.middleware.push(middleware);
        self
    }
}

/// 节点执行上下文。
///
/// - `config`：节点上不通过 pin 传入的字面量配置。
/// - `host`：宿主服务（日志 / 变量）。
/// - `cancel`：协作取消令牌；长时操作的节点应定期 `cancel.is_cancelled()` 自检。
/// - `chunk_tx`：可选的流式输出通道。若为 `None`，节点应当退化为非流式行为。
/// - `permissions` / `budget` / `trace`：可选通用能力，由外层系统适配注入。
/// - `external`：透明外部上下文，供适配层 downcast；核心引擎不解释其语义。
pub struct InvokeCtx<'a> {
    pub config: &'a HashMap<String, Value>,
    pub host: &'a mut dyn HostServices,
    pub cancel: &'a CancellationToken,
    pub chunk_tx: Option<&'a ChunkTx>,
    pub permissions: Option<&'a dyn PermissionGate>,
    pub budget: Option<&'a dyn BudgetMeter>,
    pub trace: Option<&'a dyn TraceSink>,
    pub external: Option<&'a (dyn Any + Send + Sync)>,
}

impl<'a> InvokeCtx<'a> {
    pub fn from_env(
        config: &'a HashMap<String, Value>,
        host: &'a mut dyn HostServices,
        fallback_cancel: &'a CancellationToken,
        env: &'a ExecutionEnv<'a>,
    ) -> Self {
        Self {
            config,
            host,
            cancel: env.cancel.unwrap_or(fallback_cancel),
            chunk_tx: env.chunk_tx,
            permissions: env.permissions,
            budget: env.budget,
            trace: env.trace,
            external: env.external,
        }
    }

    /// 是否处于流式模式。
    pub fn is_streaming(&self) -> bool {
        self.chunk_tx.is_some()
    }

    /// 便利方法：尝试发送一个 chunk；非流式模式下返回 false。
    /// 异步版本在 [`crate::registry::runner`] 中以 helper 形式提供。
    pub fn try_send_chunk(&self, chunk: Chunk) -> bool {
        match self.chunk_tx {
            Some(tx) => tx.try_send(chunk).is_ok(),
            None => false,
        }
    }

    pub fn check_permission(&self, action: &str, scope: &str) -> VsResult<()> {
        if let Some(permissions) = self.permissions {
            permissions.check_permission(action, scope)?;
        }
        Ok(())
    }

    pub fn consume_budget(&self, dimension: &str, amount: f64) -> VsResult<()> {
        if let Some(budget) = self.budget {
            budget.consume_budget(dimension, amount)?;
        }
        Ok(())
    }

    pub fn record_trace(&self, event: &str, attrs: &[(&str, Value)]) {
        if let Some(trace) = self.trace {
            trace.record_trace(event, attrs);
        }
    }

    pub fn external<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.external.and_then(|external| external.downcast_ref::<T>())
    }
}

/// 内存实现：测试与单机用途。
#[derive(Default)]
pub struct InMemoryHost {
    pub vars: HashMap<String, Value>,
    pub log_buffer: Vec<(LogLevel, String)>,
}

impl HostServices for InMemoryHost {
    fn log(&mut self, level: LogLevel, msg: &str) {
        self.log_buffer.push((level, msg.to_string()));
    }
    fn var_get(&self, name: &str) -> Option<Value> {
        self.vars.get(name).cloned()
    }
    fn var_set(&mut self, name: &str, value: Value) {
        self.vars.insert(name.to_string(), value);
    }
}
