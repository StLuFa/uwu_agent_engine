//! visual_script NodeDefinition registration — `"perception.observe"` (Impure + Async)

use std::sync::Arc;
use uwu_visual_script::prelude::*;

fn exec_in() -> Pin { Pin { name: "exec_in".into(), dir: PinDir::In, ty: ValueType::Exec, default: None } }
fn exec_out(name: &str) -> Pin { Pin { name: name.into(), dir: PinDir::Out, ty: ValueType::Exec, default: None } }
fn data_in(name: &str, ty: ValueType) -> Pin { Pin { name: name.into(), dir: PinDir::In, ty, default: None } }
fn data_out(name: &str, ty: ValueType) -> Pin { Pin { name: name.into(), dir: PinDir::Out, ty, default: None } }

pub fn register_nodes(lib: &mut NodeLibrary) {
    lib.register(NodeDefinition {
        id: "perception.observe".into(),
        purity: Purity::Impure,
        inputs: vec![exec_in(), data_in("raw_input", ValueType::String)],
        outputs: vec![exec_out("exec"), data_out("context", ValueType::String)],
        runner: RunnerKind::Async(Arc::new(PerceptionRunner)),
    });
}

struct PerceptionRunner;
#[async_trait::async_trait]
impl AsyncNodeRunner for PerceptionRunner {
    async fn invoke(&self, inputs: &[Value], outputs: &mut [Value], ctx: &mut InvokeCtx<'_>) -> VsResult<ExecNext> {
        let raw = inputs.get(1).and_then(|v| v.as_str()).unwrap_or("");
        outputs[1] = Value::String(format!("parsed: {raw}").into());
        let _ = ctx;
        Ok(ExecNext::Pin("exec".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_registers_in_library() {
        let mut lib = NodeLibrary::new();
        register_nodes(&mut lib);

        let def = lib.get("perception.observe").expect("node should be registered");
        assert_eq!(def.id, "perception.observe");
        assert_eq!(def.purity, Purity::Impure);
        assert_eq!(def.inputs.len(), 2); // exec_in + raw_input
        assert_eq!(def.outputs.len(), 2); // exec_out + context
        assert!(!def.is_event()); // has exec_in → not an event entry node
    }

    #[test]
    fn node_runner_is_async() {
        let mut lib = NodeLibrary::new();
        register_nodes(&mut lib);
        let def = lib.get("perception.observe").unwrap();
        assert!(def.runner.is_async());
    }
}
