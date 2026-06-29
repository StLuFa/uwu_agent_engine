//! SlotProgram 解释器 —— 同步 + 异步双路径。
//!
//! 主循环：当前 block -> 顺序执行 instr -> 遇到 CallImpure 时根据 ExecNext
//! 跳到下一 block；其余 instr 都在 block 内顺序计算完毕。
//!
//! 同一个 [`SlotProgram`] 可以混合同步 / 异步 runner：
//! - [`Vm::run_entry`] / [`Vm::run_all`] 只能跑全部为 [`crate::RunnerKind::Sync`]
//!   的程序；遇到 [`crate::RunnerKind::Async`] 节点会返回 [`VsError::AsyncRunnerInSyncVm`]。
//! - [`Vm::run_entry_async`] / [`Vm::run_all_async`] 两种 runner 都能跑，
//!   并且可以接收一个可选的 `ChunkTx` 用于流式输出。

use crate::error::{VsError, VsResult};
use crate::ir::{BlockId, Instr, SlotProgram};
use crate::registry::{
    ChunkTx, ExecNext, ExecutionEnv, HostServices, InvokeCtx, NodeCallInfo, NodePhase, RunnerKind,
};
use crate::value::Value;
use tokio_util::sync::CancellationToken;

/// VM 默认 step budget。可通过 [`Vm::with_step_budget`] 覆盖。
pub const DEFAULT_STEP_BUDGET: u64 = 1_000_000;

pub struct Vm {
    program: SlotProgram,
    step_budget: u64,
}

impl Vm {
    pub fn new(program: SlotProgram) -> Self {
        Self { program, step_budget: DEFAULT_STEP_BUDGET }
    }

    /// 设置 step budget；0 表示不限制（仅推荐受信场景）。
    pub fn with_step_budget(mut self, budget: u64) -> Self {
        self.step_budget = budget;
        self
    }

    pub fn program(&self) -> &SlotProgram {
        &self.program
    }

    // ─────────────────────────── 同步入口 ───────────────────────────

    /// 同步执行：从指定 entry 节点出发。仅支持 Sync runner。
    pub fn run_entry(&self, entry_node: u32, host: &mut dyn HostServices) -> VsResult<()> {
        let cancel = CancellationToken::new();
        let env = ExecutionEnv::new().with_cancel(&cancel);
        self.run_entry_with_env(entry_node, host, &env)
    }

    /// 使用外部执行环境同步执行指定 entry。
    pub fn run_entry_with_env(
        &self,
        entry_node: u32,
        host: &mut dyn HostServices,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        let block = self.entry_block(entry_node)?;
        let mut slots: Vec<Value> = vec![Value::Unit; self.program.slots_count as usize];
        let fallback_cancel = CancellationToken::new();
        self.run_from_sync(block, &mut slots, host, &fallback_cancel, env)
    }

    /// 同步执行所有 entry。
    pub fn run_all(&self, host: &mut dyn HostServices) -> VsResult<()> {
        let cancel = CancellationToken::new();
        let env = ExecutionEnv::new().with_cancel(&cancel);
        self.run_all_with_env(host, &env)
    }

    /// 使用外部执行环境同步执行所有 entry。
    pub fn run_all_with_env(
        &self,
        host: &mut dyn HostServices,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        for &entry in self.program.entries.keys() {
            self.run_entry_with_env(entry, host, env)?;
        }
        Ok(())
    }

    // ─────────────────────────── 异步入口 ───────────────────────────

    /// 异步执行：从指定 entry 节点出发。`chunk_tx` 不为 `None` 时即为流式模式，
    /// 节点可通过 [`crate::InvokeCtx::chunk_tx`] 推送 [`crate::Chunk`]。
    pub async fn run_entry_async(
        &self,
        entry_node: u32,
        host: &mut dyn HostServices,
        cancel: &CancellationToken,
        chunk_tx: Option<&ChunkTx>,
    ) -> VsResult<()> {
        let mut env = ExecutionEnv::new().with_cancel(cancel);
        if let Some(chunk_tx) = chunk_tx {
            env = env.with_chunk_tx(chunk_tx);
        }
        self.run_entry_async_with_env(entry_node, host, &env).await
    }

    /// 使用外部执行环境异步执行指定 entry。
    pub async fn run_entry_async_with_env(
        &self,
        entry_node: u32,
        host: &mut dyn HostServices,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        let block = self.entry_block(entry_node)?;
        let mut slots: Vec<Value> = vec![Value::Unit; self.program.slots_count as usize];
        let fallback_cancel = CancellationToken::new();
        self.run_from_async(block, &mut slots, host, &fallback_cancel, env).await
    }

    /// 异步执行所有 entry。
    pub async fn run_all_async(
        &self,
        host: &mut dyn HostServices,
        cancel: &CancellationToken,
        chunk_tx: Option<&ChunkTx>,
    ) -> VsResult<()> {
        let mut env = ExecutionEnv::new().with_cancel(cancel);
        if let Some(chunk_tx) = chunk_tx {
            env = env.with_chunk_tx(chunk_tx);
        }
        self.run_all_async_with_env(host, &env).await
    }

    /// 使用外部执行环境异步执行所有 entry。
    pub async fn run_all_async_with_env(
        &self,
        host: &mut dyn HostServices,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        let entries: Vec<u32> = self.program.entries.keys().copied().collect();
        for entry in entries {
            self.run_entry_async_with_env(entry, host, env).await?;
        }
        Ok(())
    }

    // ─────────────────────────── 内部 ───────────────────────────

    fn entry_block(&self, entry_node: u32) -> VsResult<BlockId> {
        self.program
            .entries
            .get(&entry_node)
            .copied()
            .ok_or_else(|| VsError::Runtime(format!("no entry block for node {}", entry_node)))
    }

    fn check_budget(&self, budget: &mut u64) -> VsResult<()> {
        if self.step_budget == 0 {
            return Ok(());
        }
        if *budget == 0 {
            return Err(VsError::Runtime("execution step budget exceeded".into()));
        }
        *budget -= 1;
        Ok(())
    }

    fn run_from_sync(
        &self,
        mut block: BlockId,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        let mut budget = self.step_budget;
        while block != SlotProgram::HALT {
            if env.cancel.unwrap_or(fallback_cancel).is_cancelled() {
                return Err(VsError::Cancelled);
            }
            self.check_budget(&mut budget)?;
            block = self.run_block_sync(block, slots, host, fallback_cancel, env)?;
        }
        Ok(())
    }

    async fn run_from_async(
        &self,
        mut block: BlockId,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<()> {
        let mut budget = self.step_budget;
        while block != SlotProgram::HALT {
            if env.cancel.unwrap_or(fallback_cancel).is_cancelled() {
                return Err(VsError::Cancelled);
            }
            self.check_budget(&mut budget)?;
            block = self
                .run_block_async(block, slots, host, fallback_cancel, env)
                .await?;
        }
        Ok(())
    }

    fn run_block_sync(
        &self,
        block: BlockId,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<BlockId> {
        // 注意：与异步版几乎一致；为了避免泛型 bloat 这里手写两份。
        let block_ref = self
            .program
            .blocks
            .get(block as usize)
            .ok_or_else(|| VsError::Runtime(format!("invalid block id {}", block)))?;
        // 借用 instrs 通过 index 推进，避免与 self.dispatch 借用冲突。
        let n = block_ref.instrs.len();
        for i in 0..n {
            // 每条 instr 单独取引用，避免长生命周期借用。
            let instr = &self.program.blocks[block as usize].instrs[i];
            match instr.clone() {
                Instr::LoadConst { dst, value } => {
                    slots[dst as usize] = value;
                }
                Instr::Move { dst, src } => {
                    slots[dst as usize] = slots[src as usize].clone();
                }
                Instr::LoadVar { dst, var } => {
                    let name = &self.program.vars[var as usize];
                    slots[dst as usize] = host.var_get(name).unwrap_or(Value::Unit);
                }
                Instr::StoreVar { var, src } => {
                    let name = self.program.vars[var as usize].clone();
                    host.var_set(&name, slots[src as usize].clone());
                }
                Instr::CallPure { def, node_id, inputs, outputs } => {
                    self.dispatch_sync(def, node_id, inputs, outputs, slots, host, fallback_cancel, env)?;
                }
                Instr::CallImpure { def, node_id, inputs, outputs, targets, on_end } => {
                    let next = self
                        .dispatch_sync(def, node_id, inputs, outputs, slots, host, fallback_cancel, env)?;
                    return Ok(resolve_next(next, &targets, on_end));
                }
                Instr::Jump { target } => return Ok(target),
            }
        }
        Ok(SlotProgram::HALT)
    }

    async fn run_block_async(
        &self,
        block: BlockId,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<BlockId> {
        let block_ref = self
            .program
            .blocks
            .get(block as usize)
            .ok_or_else(|| VsError::Runtime(format!("invalid block id {}", block)))?;
        let n = block_ref.instrs.len();
        for i in 0..n {
            let instr = self.program.blocks[block as usize].instrs[i].clone();
            match instr {
                Instr::LoadConst { dst, value } => {
                    slots[dst as usize] = value;
                }
                Instr::Move { dst, src } => {
                    slots[dst as usize] = slots[src as usize].clone();
                }
                Instr::LoadVar { dst, var } => {
                    let name = &self.program.vars[var as usize];
                    slots[dst as usize] = host.var_get(name).unwrap_or(Value::Unit);
                }
                Instr::StoreVar { var, src } => {
                    let name = self.program.vars[var as usize].clone();
                    host.var_set(&name, slots[src as usize].clone());
                }
                Instr::CallPure { def, node_id, inputs, outputs } => {
                    self.dispatch_async(
                        def, node_id, inputs, outputs, slots, host, fallback_cancel, env,
                    )
                    .await?;
                }
                Instr::CallImpure { def, node_id, inputs, outputs, targets, on_end } => {
                    let next = self
                        .dispatch_async(
                            def, node_id, inputs, outputs, slots, host, fallback_cancel, env,
                        )
                        .await?;
                    return Ok(resolve_next(next, &targets, on_end));
                }
                Instr::Jump { target } => return Ok(target),
            }
        }
        Ok(SlotProgram::HALT)
    }

    fn dispatch_sync(
        &self,
        def: u32,
        node_id: u32,
        inputs: std::ops::Range<u32>,
        outputs: std::ops::Range<u32>,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<ExecNext> {
        let def = self.program.defs[def as usize].clone();
        let in_vals: Vec<Value> =
            slots[inputs.start as usize..inputs.end as usize].to_vec();
        let mut out_vals: Vec<Value> =
            slots[outputs.start as usize..outputs.end as usize].to_vec();
        let empty_config = std::collections::HashMap::new();
        let mut ctx = InvokeCtx::from_env(&empty_config, host, fallback_cancel, env);
        self.run_middleware(node_id, &def.id, NodePhase::Before, env, &mut ctx)?;
        let next = match &def.runner {
            RunnerKind::Sync(r) => r.invoke(&in_vals, &mut out_vals, &mut ctx)?,
            RunnerKind::Async(_) => return Err(VsError::AsyncRunnerInSyncVm),
        };
        self.run_middleware(node_id, &def.id, NodePhase::After, env, &mut ctx)?;
        for (i, v) in out_vals.into_iter().enumerate() {
            slots[outputs.start as usize + i] = v;
        }
        Ok(next)
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch_async(
        &self,
        def: u32,
        node_id: u32,
        inputs: std::ops::Range<u32>,
        outputs: std::ops::Range<u32>,
        slots: &mut [Value],
        host: &mut dyn HostServices,
        fallback_cancel: &CancellationToken,
        env: &ExecutionEnv<'_>,
    ) -> VsResult<ExecNext> {
        let def = self.program.defs[def as usize].clone();
        let in_vals: Vec<Value> =
            slots[inputs.start as usize..inputs.end as usize].to_vec();
        let mut out_vals: Vec<Value> =
            slots[outputs.start as usize..outputs.end as usize].to_vec();
        let empty_config = std::collections::HashMap::new();
        let mut ctx = InvokeCtx::from_env(&empty_config, host, fallback_cancel, env);
        self.run_middleware(node_id, &def.id, NodePhase::Before, env, &mut ctx)?;
        let next = match &def.runner {
            RunnerKind::Sync(r) => r.invoke(&in_vals, &mut out_vals, &mut ctx)?,
            RunnerKind::Async(r) => r.invoke(&in_vals, &mut out_vals, &mut ctx).await?,
        };
        self.run_middleware(node_id, &def.id, NodePhase::After, env, &mut ctx)?;
        for (i, v) in out_vals.into_iter().enumerate() {
            slots[outputs.start as usize + i] = v;
        }
        Ok(next)
    }

    fn run_middleware(
        &self,
        node_id: u32,
        def_id: &str,
        phase: NodePhase,
        env: &ExecutionEnv<'_>,
        ctx: &mut InvokeCtx<'_>,
    ) -> VsResult<()> {
        for middleware in &env.middleware {
            middleware.on_node(NodeCallInfo { node_id, def_id, phase }, ctx)?;
        }
        Ok(())
    }
}

fn resolve_next(next: ExecNext, targets: &[(String, BlockId)], on_end: BlockId) -> BlockId {
    match next {
        ExecNext::Pin(name) => targets
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, b)| *b)
            .unwrap_or(on_end),
        ExecNext::End => on_end,
    }
}
