//! 常用类型一站式 re-export。
//!
//! ```ignore
//! use uwu_visual_script::prelude::*;
//! ```

pub use crate::compiler::compile;
pub use crate::error::{VsError, VsResult};
pub use crate::ir::{Instr, SlotProgram};
pub use crate::model::{Edge, Endpoint, Graph, Node, NodeDefRef, NodeId, Pin, PinDir, Variable};
pub use crate::registry::{
    AsyncNodeRunner, BudgetMeter, Chunk, ChunkTx, ExecNext, ExecutionEnv, FnRunner, HostServices,
    InMemoryHost, InvokeCtx, LogLevel, NodeCallInfo, NodeDefinition, NodeLibrary, NodeMiddleware,
    NodePhase, NodeRunner, PermissionGate, Purity, RunnerKind, TraceSink, send_chunk,
};
pub use crate::value::{Value, ValueType};
pub use crate::vm::Vm;
