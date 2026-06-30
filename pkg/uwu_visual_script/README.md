# uwu_visual_script

可视化脚本引擎 —— Graph 编排 + 扁平 IR + 解释器，
为「事件驱动、节点可热替换、可审计、可嵌入」的本地脚本场景打底。

不假设运行环境：节点既可以是纯 Rust 闭包（内置），也可以是
[`uwu_wasm`](../uwu_wasm/) 沙箱里的 WASM Component（实现一个 `NodeRunner`
trait 即可接入）。编译器与 VM 对后端透明，同一个 Graph 可混用 Rust / Wasm /
未来的 RPC 节点。

---

## 特性

- **双流模型**：exec 控制流 + data 数据流
- **同步 / 异步双轨**：`NodeRunner`（sync）+ `AsyncNodeRunner`（async/`.await`），`RunnerKind` 二选一挂在 `NodeDefinition`；`Vm::run_*` 走纯同步路径，`Vm::run_*_async` 兼容两种 runner
- **节点级流式输出**：`InvokeCtx::chunk_tx` 暴露 `mpsc::Sender<Chunk>`，节点可推 `Delta` / `Progress` / `Final`；非流式调用方传 `None` 即可
- **协作取消**：`InvokeCtx::cancel: &CancellationToken`，VM 主循环每个 block 自检；节点长时操作应自行 `cancel.cancelled().await`
- **通用执行环境**：`ExecutionEnv` 可注入 `PermissionGate` / `BudgetMeter` / `TraceSink` / `NodeMiddleware`，由上层桥接权限、预算、trace，`uvs` 不依赖 `nono_ability`
- **三层 IR**：`Graph`（编辑器）→ `LowerCtx`（编译期）→ `SlotProgram`（运行期扁平指令）
- **静态校验**：方向 / exec-data 一致性 / 类型兼容 / Wildcard 拒绝 / Pure 子图无环
- **Pure 节点折叠**：每个 impure block 内反向递归 emit `CallPure`，编译期消除重复求值
- **slot 数组运行期**：`SlotId` 直接当下标，VM 主循环零 HashMap、无递归驱动
- **可插拔执行体**：`NodeRunner` trait → `FnRunner` / `WasmRunner`（待补）/ 自定义
- **隐式类型转换**：`I32 → F64`、`* → String` 等内建在 `ValueType::accepts` / `Value::coerce`
- **事件入口自动识别**：无 exec 入边的 Impure event 节点自动注册为 entry
- **HostServices 抽象**：日志 / 变量读写解耦，默认 `InMemoryHost`，可换 `nono_memory`
- **运行步数预算**：VM 主循环内置 step budget，防止误编译产生死循环

非目标：节点 UI 渲染、磁盘资产管线、领域逻辑节点库 —— 这些应在更上层构建。

---

## 模块布局

```
src/
├── lib.rs            纯 facade + 顶层 re-export
├── prelude.rs        一站式 re-export
├── value.rs          Value / ValueType（与 wasm component 类型对齐）
├── model.rs          Graph / Node / Edge / Pin / Variable / Endpoint
├── ir.rs             SlotProgram / Instr / Block（扁平指令 IR）
├── error.rs          VsError / VsResult
├── vm.rs             解释器
├── registry/         节点注册表
│   ├── runner.rs         NodeRunner trait + FnRunner
│   ├── host.rs           HostServices + InvokeCtx + ExecutionEnv + capability traits
│   └── library.rs        NodeDefinition / NodeLibrary / Purity / ExecNext
├── compiler/         Graph -> SlotProgram
│   ├── context.rs        LowerCtx（私有共享上下文）
│   ├── validate.rs       类型 / Wildcard / 循环检测
│   └── lower.rs          impure 节点 -> Block，pure 反向折叠为 CallPure
└── builtin/          内置节点
    ├── pins.rs           exec_in / exec_out / data_in / data_out 助手
    ├── events.rs         event.begin
    ├── flow.rs           flow.branch / flow.passthrough
    ├── math.rs           math.add / math.mul
    ├── compare.rs        cmp.greater
    ├── debug.rs          debug.print
    └── vars.rs           var.inc_counter
```

---

## 快速开始

```rust
use uwu_visual_script::prelude::*;
use std::collections::HashMap;

// 1. 节点库（含全部内置节点）
let lib = NodeLibrary::with_builtins();

// 2. 拼一个 Graph：begin --> branch --true--> print
//                              \--false-> print
let mk = |id, def: &str| Node {
    id, def: NodeDefRef { id: def.into(), version: None },
    title: None, config: HashMap::new(),
};
let ep = |node, pin| Endpoint { node, pin };

let mut g = Graph::default();
g.name = "demo".into();
g.nodes = vec![
    mk(1, "event.begin"),
    mk(2, "flow.branch"),
    mk(3, "debug.print"),
    mk(4, "debug.print"),
];
g.edges = vec![
    Edge { from: ep(1, 0), to: ep(2, 0) },   // begin.then  -> branch.exec_in
    Edge { from: ep(2, 0), to: ep(3, 0) },   // branch.true -> print3.exec_in
    Edge { from: ep(2, 1), to: ep(4, 0) },   // branch.false-> print4.exec_in
];
g.entries = vec![1];

// 3. 编译并执行
let program = compile(&g, &lib)?;
let mut host = InMemoryHost::default();
Vm::new(program).run_all(&mut host)?;
```

---

## 核心 API

### 类型与值

```rust
ValueType::{Bool, I32, I64, F32, F64, String, List(_), Wildcard, Exec}
Value::{Unit, Bool, I32, I64, F32, F64, String(Arc<str>), List(Arc<Vec<_>>)}

let v = Value::I32(42).coerce(&ValueType::F64).unwrap();   // 隐式转换
ValueType::F64.accepts(&ValueType::I32);                    // true
```

数值小标量内联，`String` / `List` 走 `Arc` 共享，避免编排链路上的不必要拷贝。

### Graph 模型

```rust
Graph    { name, nodes, edges, variables, entries }
Node     { id: NodeId, def: NodeDefRef, title, config: HashMap<String, Value> }
Pin      { name, dir: PinDir, ty: ValueType, default: Option<Value> }
Edge     { from: Endpoint, to: Endpoint }
Endpoint { node: NodeId, pin: PinIndex }
Variable { name, ty, default }
```

`config` 为节点上不通过 pin 传入的字面量配置；`default` 为未连线的 In-Data
pin 的回退值。

### 节点注册表

```rust
let mut lib = NodeLibrary::new();
lib.register(NodeDefinition {
    id: "math.square".into(),
    purity: Purity::Pure,
    inputs:  vec![ /* Pin { ty: F64, .. } */ ],
    outputs: vec![ /* Pin { ty: F64, .. } */ ],
    runner: Arc::new(FnRunner(|inp, out, _cx| {
        let x = inp[0].as_f64().unwrap_or(0.0);
        out[0] = Value::F64(x * x);
        Ok(ExecNext::End)
    })),
});
```

`NodeLibrary::with_builtins()` 一键注册全部内置节点。

### 编译与运行

```rust
let program: SlotProgram = compile(&graph, &lib)?;
let vm = Vm::new(program);

vm.run_entry(entry_node_id, &mut host)?;   // 单入口（sync）
vm.run_all(&mut host)?;                    // 所有 entry（sync）

// 异步 / 流式 / 取消
use tokio_util::sync::CancellationToken;
let cancel = CancellationToken::new();
let (tx, mut rx) = tokio::sync::mpsc::channel::<Chunk>(16);
vm.run_all_async(&mut host, &cancel, Some(&tx)).await?;
// 另一侧消费：while let Some(chunk) = rx.recv().await { ... }
```

VM 主循环每跑一个 block 就消耗一格 step budget（默认 1,000,000，可通过
`Vm::with_step_budget` 覆盖；0 表示不限），同时检查 `cancel.is_cancelled()`，
任何一项触发都会立即返回（`VsError::Runtime` / `VsError::Cancelled`）。

> 同步 VM 路径遇到 `RunnerKind::Async` 节点会返回 `VsError::AsyncRunnerInSyncVm`；
> 异步 VM 路径同时支持两种 runner，可逐步把存量同步节点替换为 async。

### HostServices

```rust
trait HostServices {
    fn log(&mut self, level: LogLevel, msg: &str);
    fn var_get(&self, name: &str) -> Option<Value>;
    fn var_set(&mut self, name: &str, value: Value);
}

let mut host = InMemoryHost::default();
host.var_set("counter", Value::F64(0.0));
```

`InvokeCtx` 在节点执行体内可读 `config`、访问 `host`，并通过可选 capability traits 使用外部能力：

```rust
let env = ExecutionEnv::new()
    .with_permissions(&permission_gate)
    .with_budget(&budget_meter)
    .with_trace(&trace_sink)
    .with_middleware(&middleware);

vm.run_all_with_env(&mut host, &env)?;
```

`uvs` 只定义 `PermissionGate` / `BudgetMeter` / `TraceSink` / `NodeMiddleware` 抽象；

---

## 内置节点

| ID | 类型 | 输入 | 输出 | 说明 |
|---|---|---|---|---|
| `event.begin` | Impure | — | `then: exec` | 默认事件入口 |
| `flow.branch` | Impure | `exec_in`, `cond: bool` | `true: exec`, `false: exec` | 条件分支 |
| `flow.passthrough` | Impure | `exec_in` | `then: exec` | 占位直通 |
| `math.add` | Pure | `a, b: f64` | `sum: f64` | 加法 |
| `math.mul` | Pure | `a, b: f64` | `product: f64` | 乘法 |
| `cmp.greater` | Pure | `a, b: f64` | `out: bool` | `a > b` |
| `debug.print` | Impure | `exec_in`, `msg: string` | `then: exec` | 写日志 |
| `var.inc_counter` | Impure | `exec_in` | `then: exec`, `after: f64` | 计数器自增（演示） |

---

## 设计要点

- **Pure / Impure 二分**：决定节点是 demand-driven 还是 exec-driven，对应 IR 中的 `CallPure` / `CallImpure`。
- **每个 impure 节点 → 一个 Block**：exec 跳转就是 Block 间的 BlockId 跳转，无递归调用栈。
- **Pure 节点反向折叠**：lower 期对每个 block 维护 `pure_emitted` 缓存，同一 pure 节点在同 block 内只算一次。
- **Slot 扁平数组**：`SlotId` 直接做下标，CPU cache 友好；编译期完成全部 slot 分配。
- **HALT sentinel**：`SlotProgram::HALT = u32::MAX`，VM 主循环遇到即返回。
- **类型边界**：`ValueType` 与 wasmtime `component::Val` 形态对齐，便于 `WasmRunner` 直接桥接。
- **Wildcard 必须编译期解析**：避免运行期再做类型协商；MVP 直接拒绝未消解的 Wildcard。

---

## 性能与扩展计划

设计文档内已铺好的优化点（按优先级）：

- **P0**：预编译 `TypedCallSite`（消除运行期 HashMap）、Pure cluster 跨 block 共享、Store 池化（Pure 节点）
- **P1**：常量折叠 + DCE、Receipt 模式分级、副作用感知并行（`Parallel` 节点）、Slot 内联 + Resource handle、变量短路、Latent 异步调度
- **P2**：Hot-Path AOT 融合（标记 `hot` 的子图编译为单个 wasm Component）、确定性回放、Snapshot 懒序列化

---

## 接入 uwu_wasm

`NodeRunner` 是 trait 对象，新增一个 `WasmRunner` 即可：

```rust
struct WasmRunner {
    sandbox: Arc<uwu_wasm::Sandbox>,
    module: String,
    export: String,
}

impl NodeRunner for WasmRunner {
    fn invoke(&self, inputs: &[Value], outputs: &mut [Value], _cx: &mut InvokeCtx<'_>)
        -> VsResult<ExecNext>
    {
        // Value -> wasmtime::component::Val 编组
        // sandbox.call_typed(&self.module, &self.export, params)
        // Val -> Value 解组写入 outputs
        Ok(ExecNext::End)
    }
}
```

编译器与 VM 对后端透明：同一个 Graph 可以混用 Rust 闭包节点、Wasm 节点、未来的 RPC 节点。

---

## 错误模型

```rust
pub enum VsError {
    UnknownDef(String),       // NodeDefRef 在 NodeLibrary 中找不到
    Type { /* … */ },         // 类型不兼容 / exec-data 误连 / Wildcard 未解析
    Cycle { /* … */ },        // Pure 子图自环
    Runtime(String),          // VM 运行期错误（含 step budget 耗尽）
    // …
}
```

编译期错误一律在 `compile()` 返回；运行期错误从 `vm.run_*` 返回。所有路径
都以 `VsResult<T>` 暴露。

---

## 测试

```bash
cargo test -p uwu_visual_script
```

当前测试：4 个。集成测试位于 `tests/integration.rs`：

- `library_registers_builtins` — 注册表自检
- `end_to_end_branch_default_false` — Begin → Branch → Print 完整流
- `impure_node_persists_state_across_runs` — Impure 节点 + Graph 变量
- `type_mismatch_is_caught` — exec/data 误连被拒
- `cycle_in_pure_subgraph_is_rejected` — Pure 子图自环被拒

---

## License

WIP — internal use.
