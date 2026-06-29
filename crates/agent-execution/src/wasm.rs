//! WASM sandbox executor — runs actions inside a WebAssembly sandbox.
//!
//! Feature-gated by `wasm-sandbox`. Wraps `uwu_wasm::Sandbox` and integrates
//! with the `Executor` trait, so it can be plugged into `agent-session` or
//! `FlowEngine` as a drop-in execution backend.
//!
//! ## Security model (inherited from uwu_wasm)
//! - Per-call micro-sandbox (fresh `Store` every invocation)
//! - Fuel budget + epoch deadline enforced by wasmtime
//! - Attestor-signed execution receipts for auditability
//! - Zero-trust capability policy (default: no host imports, 16 MiB memory, 1 s deadline)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agent_state::AgentState;
use agent_types_core::Action;
use async_trait::async_trait;
use uwu_wasm::{Attestor, ModuleSource, Policy, Sandbox, SandboxEngine};

use super::{ExecutionResult, Executor};

/// Information about a registered WASM module.
struct ModuleEntry {
    /// The exported function name to call (e.g. `"add"`).
    export_func: String,
}

/// Executes actions inside a WebAssembly sandbox.
///
/// ## Action → WASM routing
/// - `action.command` selects the registered module
/// - `action.params` fields `"a"` and `"b"` are extracted as `i32` args
/// - Result is formatted as `"<i32_return_value>"`
///
/// ## Example
/// ```ignore
/// let mut exec = WasmExecutor::new()?;
/// exec.register_module("math", adder_wasm_bytes, "add")?;
///
/// let action = Action::new("math", ActionParams::new().with("a", 3).with("b", 4));
/// let result = exec.execute(&action, &state).await;
/// assert_eq!(result.output, "7");
/// ```
pub struct WasmExecutor {
    engine: Arc<SandboxEngine>,
    sandbox: Arc<Sandbox>,
    attestor: Arc<Attestor>,
    policy: Policy,
    modules: HashMap<String, ModuleEntry>,
    max_parallel: usize,
}

impl WasmExecutor {
    /// Create a new `WasmExecutor` with a sensible default policy.
    ///
    /// Default policy: 1M fuel, 500 ms deadline, 64 pages (4 MiB) memory.
    pub fn new() -> anyhow::Result<Self> {
        let engine = Arc::new(SandboxEngine::new()?);
        let attestor = Arc::new(Attestor::ephemeral());
        let policy = Policy::builder()
            .fuel(1_000_000)
            .deadline(Duration::from_millis(500))
            .memory_pages(64)
            .build();
        let sandbox = Arc::new(Sandbox::new(
            "wasm-executor",
            engine.clone(),
            policy.clone(),
            attestor.clone(),
        ));

        Ok(Self {
            engine,
            sandbox,
            attestor,
            policy,
            modules: HashMap::new(),
            max_parallel: 8,
        })
    }

    /// Replace the default security policy and rebuild the sandbox.
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.sandbox = Arc::new(Sandbox::new(
            "wasm-executor",
            self.engine.clone(),
            policy.clone(),
            self.attestor.clone(),
        ));
        self.policy = policy;
        self
    }

    /// Cap the maximum number of parallel actions in `execute_batch`.
    pub fn with_max_parallel(mut self, n: usize) -> Self {
        self.max_parallel = n;
        self
    }

    /// Register a WASM module under a logical name.
    ///
    /// `name` is the key that `action.command` will match against.
    /// `wasm_bytes` must be a valid Component Model `.wasm` binary.
    /// `export_func` is the exported function that takes `(s32, s32) → s32`.
    pub fn register_module(
        &mut self,
        name: &str,
        wasm_bytes: Vec<u8>,
        export_func: &str,
    ) -> anyhow::Result<()> {
        let src = ModuleSource::new(name, wasm_bytes);
        self.engine.install(src, &self.policy)?;
        self.modules.insert(
            name.to_string(),
            ModuleEntry {
                export_func: export_func.to_string(),
            },
        );
        Ok(())
    }

    /// Extract `i32` args from action params. Returns `(arg0, arg1)` with
    /// defaults of 0 for missing keys.
    fn extract_args(action: &Action) -> (i32, i32) {
        let a = action
            .params
            .get("a")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0);
        let b = action
            .params
            .get("b")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0);
        (a, b)
    }
}

#[async_trait]
impl Executor for WasmExecutor {
    async fn execute(&self, action: &Action, _state: &AgentState) -> ExecutionResult {
        let start = std::time::Instant::now();

        let Some(entry) = self.modules.get(&action.command) else {
            let elapsed = start.elapsed().as_millis() as u64;
            return ExecutionResult {
                action: action.clone(),
                success: false,
                output: format!("unknown module: {}", action.command),
                state_delta: None,
                tokens_used: 0,
                time_elapsed_ms: elapsed,
            };
        };

        let (a, b) = Self::extract_args(action);

        match self
            .sandbox
            .call_typed::<(i32, i32), (i32,)>(&action.command, &entry.export_func, (a, b))
        {
            Ok(receipt) => {
                let elapsed = start.elapsed().as_millis() as u64;
                ExecutionResult {
                    action: action.clone(),
                    success: true,
                    output: format!("{}", receipt.returns.0),
                    state_delta: None,
                    tokens_used: receipt.fuel_consumed.unwrap_or(0),
                    time_elapsed_ms: elapsed,
                }
            }
            Err(e) => {
                let elapsed = start.elapsed().as_millis() as u64;
                ExecutionResult {
                    action: action.clone(),
                    success: false,
                    output: format!("sandbox error: {e}"),
                    state_delta: None,
                    tokens_used: 0,
                    time_elapsed_ms: elapsed,
                }
            }
        }
    }
}

impl WasmExecutor {
    /// Execute multiple actions in sequence (bounded by `max_parallel`).
    pub async fn execute_batch(
        &self,
        actions: &[Action],
        state: &AgentState,
    ) -> Vec<ExecutionResult> {
        let mut results = Vec::with_capacity(actions.len().min(self.max_parallel));
        for action in actions.iter().take(self.max_parallel) {
            results.push(self.execute(action, state).await);
        }
        results
    }
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use agent_types_core::ActionParams;

    /// Build a minimal WAT component that exports `(s32, s32) → s32`.
    /// `op` is the core instruction: `"i32.add"`, `"i32.mul"`, `"i32.sub"`.
    fn make_component(op: &str) -> Vec<u8> {
        let wat = format!(
            r#"(component
  (core module $m
    (func (export "op") (param i32 i32) (result i32)
      local.get 0 local.get 1 {op}))
  (core instance $i (instantiate $m))
  (func (export "calculate") (param "a" s32) (param "b" s32) (result s32)
    (canon lift (core func $i "op"))))"#
        );
        wat::parse_str(&wat).expect("valid WAT")
    }

    fn make_executor() -> WasmExecutor {
        WasmExecutor::new().expect("create WasmExecutor")
    }

    #[test]
    fn create_executor_with_default_policy() {
        let exec = make_executor();
        assert_eq!(exec.max_parallel, 8);
        assert!(exec.modules.is_empty());
    }

    #[test]
    fn register_module_succeeds() {
        let mut exec = make_executor();
        let bytes = make_component("i32.add");
        exec.register_module("adder", bytes, "calculate")
            .expect("register should succeed");
        assert!(exec.modules.contains_key("adder"));
    }

    #[test]
    fn custom_policy_overrides_defaults() {
        let exec = WasmExecutor::new()
            .expect("create")
            .with_policy(
                Policy::builder()
                    .fuel(500_000)
                    .deadline(Duration::from_millis(100))
                    .build(),
            );
        // Policy applied — internal sandbox rebuilt (no direct assertion
        // on policy fields, but construction succeeds).
        assert_eq!(exec.max_parallel, 8);
    }

    #[test]
    fn with_max_parallel_caps_batch() {
        let exec = make_executor().with_max_parallel(3);
        assert_eq!(exec.max_parallel, 3);
    }

    #[tokio::test]
    async fn execute_adder_module() {
        let mut exec = make_executor();
        let bytes = make_component("i32.add");
        exec.register_module("adder", bytes, "calculate")
            .expect("register");

        let action = Action::new(
            "adder",
            ActionParams::new().with("a", 11).with("b", 2),
        );
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(result.success, "should succeed: {}", result.output);
        assert_eq!(result.output, "13");
        assert!(result.time_elapsed_ms < 1000);
    }

    #[tokio::test]
    async fn execute_mul_module() {
        let mut exec = make_executor();
        let bytes = make_component("i32.mul");
        exec.register_module("mul", bytes, "calculate")
            .expect("register");

        let action = Action::new("mul", ActionParams::new().with("a", 6).with("b", 7));
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(result.success, "should succeed: {}", result.output);
        assert_eq!(result.output, "42");
    }

    #[tokio::test]
    async fn execute_sub_module() {
        let mut exec = make_executor();
        let bytes = make_component("i32.sub");
        exec.register_module("sub", bytes, "calculate")
            .expect("register");

        let action = Action::new("sub", ActionParams::new().with("a", 10).with("b", 3));
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(result.success);
        assert_eq!(result.output, "7");
    }

    #[tokio::test]
    async fn missing_params_default_to_zero() {
        let mut exec = make_executor();
        let bytes = make_component("i32.add");
        exec.register_module("add", bytes, "calculate")
            .expect("register");

        // Only provide "a", "b" defaults to 0
        let action = Action::new("add", ActionParams::new().with("a", 5));
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(result.success);
        assert_eq!(result.output, "5"); // 5 + 0
    }

    #[tokio::test]
    async fn unknown_module_returns_error() {
        let exec = make_executor();
        let action = Action::new("nonexistent", ActionParams::new());
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(!result.success);
        assert!(result.output.contains("unknown module"));
    }

    #[tokio::test]
    async fn execute_batch_respects_max_parallel() {
        let mut exec = make_executor().with_max_parallel(2);
        let bytes = make_component("i32.add");
        exec.register_module("add", bytes, "calculate")
            .expect("register");

        let actions: Vec<_> = (0..5)
            .map(|i| {
                Action::new(
                    "add",
                    ActionParams::new().with("a", i as i64).with("b", 1),
                )
            })
            .collect();
        let state = AgentState::new();

        let results = exec.execute_batch(&actions, &state).await;
        assert_eq!(results.len(), 2); // capped at max_parallel
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn strict_policy_enforced_execution_succeeds() {
        // Verify that a policy with tight (but sufficient) constraints works.
        // Fuel exhaustion edge cases are tested upstream in uwu_wasm.
        let mut exec = WasmExecutor::new()
            .expect("create")
            .with_policy(
                Policy::builder()
                    .fuel(100_000)
                    .deadline(Duration::from_millis(200))
                    .memory_pages(4)
                    .build(),
            );

        let bytes = make_component("i32.add");
        exec.register_module("add", bytes, "calculate")
            .expect("register");

        let action = Action::new("add", ActionParams::new().with("a", 7).with("b", 8));
        let state = AgentState::new();

        let result = exec.execute(&action, &state).await;
        assert!(result.success, "should succeed with reasonable fuel: {}", result.output);
        assert_eq!(result.output, "15");
    }
}
