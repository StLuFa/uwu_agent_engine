//! uwu_visual_script —— 可视化脚本引擎。
//!
//! # 模块布局
//! ```text
//! src/
//!   ├─ value.rs       Value / ValueType（与 wasm component 类型对齐）
//!   ├─ model.rs       Graph / Node / Edge / Pin / Variable（编辑器/磁盘形态）
//!   ├─ ir.rs          SlotProgram / Instr / Block（扁平指令 IR）
//!   ├─ error.rs       VsError / VsResult
//!   ├─ registry/      节点注册表
//!   │    ├─ runner.rs    NodeRunner trait + FnRunner
//!   │    ├─ host.rs      HostServices + InvokeCtx + InMemoryHost
//!   │    └─ library.rs   NodeDefinition / NodeLibrary / Purity / ExecNext
//!   ├─ compiler/      Graph -> SlotProgram
//!   │    ├─ context.rs   LowerCtx（私有）
//!   │    ├─ validate.rs  类型 / Wildcard / Pure 子图无环
//!   │    └─ lower.rs     impure 节点 -> Block，pure 反向折叠为 CallPure
//!   ├─ vm.rs          解释器
//!   ├─ builtin/       内置节点（events/flow/math/compare/debug/vars/pins）
//!   └─ prelude.rs     常用 re-export
//! ```
//!
//! # 接入 [`uwu_wasm`]
//! 实现 `WasmRunner: NodeRunner`，在 `invoke` 内把 `&[Value]` 编组为
//! `wasmtime::component::Val`，调用 `Sandbox::call_typed`，再把返回值解组
//! 回 `Value`。本 crate 的接口与具体执行后端解耦。

pub mod builtin;
pub mod compiler;
pub mod error;
pub mod ir;
pub mod model;
pub mod prelude;
pub mod registry;
pub mod value;
pub mod vm;

pub use compiler::{compile, compile_to_plan, instantiate, plan};
pub use error::{VsError, VsResult};
pub use ir::{Block, ExecutionPlan, Instr, SlotProgram};
pub use model::{Edge, Endpoint, Graph, Node, NodeDefRef, NodeId, Pin, PinDir, PinIndex, Variable};
pub use registry::{
    AsyncNodeRunner, BudgetMeter, Chunk, ChunkTx, ExecNext, ExecutionEnv, FnRunner, HostServices,
    InMemoryHost, InvokeCtx, LogLevel, NodeCallInfo, NodeDefinition, NodeLibrary, NodeMiddleware,
    NodePhase, NodeRegistry, NodeRunner, PermissionGate, Purity, RunnerKind, TraceSink,
    send_chunk,
};
pub use value::{Value, ValueType};
pub use vm::Vm;
