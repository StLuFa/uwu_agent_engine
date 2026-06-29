//! 端到端集成测试。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uwu_visual_script::prelude::*;
use uwu_visual_script::PinIndex;

fn make_node(id: NodeId, def: &str) -> Node {
    Node {
        id,
        def: NodeDefRef { id: def.into(), version: None },
        title: None,
        config: HashMap::new(),
    }
}
fn ep(node: NodeId, pin: PinIndex) -> Endpoint {
    Endpoint { node, pin }
}
fn edge(from: Endpoint, to: Endpoint) -> Edge {
    Edge { from, to }
}

struct ContextProbe;

impl NodeRunner for ContextProbe {
    fn invoke(
        &self,
        _inputs: &[Value],
        _outputs: &mut [Value],
        ctx: &mut InvokeCtx<'_>,
    ) -> VsResult<ExecNext> {
        ctx.check_permission("graph.run", "demo")?;
        ctx.consume_budget("nodes", 1.0)?;
        ctx.record_trace("probe.invoke", &[("node", Value::String(Arc::from("probe")))]);
        Ok(ExecNext::End)
    }
}

#[derive(Default)]
struct ProbeEnv {
    permissions: Mutex<Vec<(String, String)>>,
    budget: Mutex<Vec<(String, f64)>>,
    traces: Mutex<Vec<String>>,
    phases: Mutex<Vec<NodePhase>>,
}

impl PermissionGate for ProbeEnv {
    fn check_permission(&self, action: &str, scope: &str) -> VsResult<()> {
        self.permissions.lock().unwrap().push((action.to_string(), scope.to_string()));
        Ok(())
    }
}

impl BudgetMeter for ProbeEnv {
    fn consume_budget(&self, dimension: &str, amount: f64) -> VsResult<()> {
        self.budget.lock().unwrap().push((dimension.to_string(), amount));
        Ok(())
    }
}

impl TraceSink for ProbeEnv {
    fn record_trace(&self, event: &str, _attrs: &[(&str, Value)]) {
        self.traces.lock().unwrap().push(event.to_string());
    }
}

impl NodeMiddleware for ProbeEnv {
    fn on_node(&self, info: NodeCallInfo<'_>, _ctx: &mut InvokeCtx<'_>) -> VsResult<()> {
        if info.def_id == "test.context_probe" {
            self.phases.lock().unwrap().push(info.phase);
        }
        Ok(())
    }
}

fn context_probe_def() -> NodeDefinition {
    NodeDefinition {
        id: "test.context_probe".into(),
        purity: Purity::Impure,
        inputs: vec![Pin {
            name: "exec_in".into(),
            dir: PinDir::In,
            ty: ValueType::Exec,
            default: None,
        }],
        outputs: vec![],
        runner: RunnerKind::sync(ContextProbe),
    }
}

#[test]
fn library_registers_builtins() {
    let lib = NodeLibrary::with_builtins();
    assert!(lib.get("math.add").is_some());
    assert!(lib.get("flow.branch").is_some());
    assert!(lib.get("event.begin").is_some());
}

#[test]
fn end_to_end_branch_default_false() {
    // begin --> branch(cond=false default) --false--> print
    //                                     --true---> print2
    let lib = NodeLibrary::with_builtins();
    let mut g = Graph::default();
    g.name = "demo".into();
    g.nodes = vec![
        make_node(1, "event.begin"),
        make_node(2, "flow.branch"),
        make_node(3, "debug.print"),
        make_node(4, "debug.print"),
    ];
    g.edges = vec![
        edge(ep(1, 0), ep(2, 0)),     // begin.then -> branch.exec_in
        edge(ep(2, 0), ep(3, 0)),     // branch.true  -> print3.exec_in
        edge(ep(2, 1), ep(4, 0)),     // branch.false -> print4.exec_in
    ];
    g.entries = vec![1];

    let program = compile(&g, &lib).expect("compile");
    assert!(program.entries.contains_key(&1));

    let mut host = InMemoryHost::default();
    Vm::new(program).run_all(&mut host).expect("run");
    // cond 默认 false → 走 false 分支 → print4 触发一次
    assert_eq!(host.log_buffer.len(), 1);
}

#[test]
fn execution_env_injects_capabilities_and_middleware() {
    let mut lib = NodeLibrary::with_builtins();
    lib.register(context_probe_def());

    let mut g = Graph::default();
    g.nodes = vec![make_node(1, "event.begin"), make_node(2, "test.context_probe")];
    g.edges = vec![edge(ep(1, 0), ep(2, 0))];
    g.entries = vec![1];

    let vm = Vm::new(compile(&g, &lib).unwrap());
    let mut host = InMemoryHost::default();
    let probe = ProbeEnv::default();
    let env = ExecutionEnv::new()
        .with_permissions(&probe)
        .with_budget(&probe)
        .with_trace(&probe)
        .with_middleware(&probe);

    vm.run_all_with_env(&mut host, &env).unwrap();

    assert_eq!(probe.permissions.lock().unwrap().as_slice(), &[("graph.run".into(), "demo".into())]);
    assert_eq!(probe.budget.lock().unwrap().as_slice(), &[("nodes".into(), 1.0)]);
    assert_eq!(probe.traces.lock().unwrap().as_slice(), &["probe.invoke".to_string()]);
    assert_eq!(probe.phases.lock().unwrap().as_slice(), &[NodePhase::Before, NodePhase::After]);
}

#[test]
fn impure_node_persists_state_across_runs() {
    let lib = NodeLibrary::with_builtins();
    let mut g = Graph::default();
    g.nodes = vec![
        make_node(1, "event.begin"),
        make_node(2, "var.inc_counter"),
    ];
    g.edges = vec![edge(ep(1, 0), ep(2, 0))];
    g.entries = vec![1];

    let vm = Vm::new(compile(&g, &lib).unwrap());
    let mut host = InMemoryHost::default();
    for _ in 0..3 {
        vm.run_all(&mut host).unwrap();
    }
    let counter = host.vars.get("counter").and_then(|v| v.as_f64()).unwrap();
    assert_eq!(counter, 3.0);
}

#[test]
fn type_mismatch_is_caught() {
    let lib = NodeLibrary::with_builtins();
    let mut g = Graph::default();
    g.nodes = vec![
        make_node(1, "event.begin"),
        make_node(2, "cmp.greater"),
    ];
    g.edges = vec![edge(ep(1, 0), ep(2, 0))]; // exec -> data
    let err = compile(&g, &lib).err().expect("expected error");
    let msg = format!("{err}");
    assert!(msg.contains("exec/data") || msg.contains("type"), "got {msg}");
}

#[test]
fn cycle_in_pure_subgraph_is_rejected() {
    let lib = NodeLibrary::with_builtins();
    let mut g = Graph::default();
    g.nodes = vec![make_node(1, "math.add")];
    g.edges = vec![edge(ep(1, 0), ep(1, 0))]; // 自环
    let err = compile(&g, &lib).err().expect("expected error");
    let msg = format!("{err}");
    assert!(msg.contains("cycle") || msg.contains("Cycle"), "got {msg}");
}
