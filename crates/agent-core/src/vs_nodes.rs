//! visual_script NodeDefinition registration — `"flow.run"` (Impure + Async)

use std::sync::Arc;
use uwu_visual_script::prelude::*;

fn exec_in() -> Pin { Pin { name: "exec_in".into(), dir: PinDir::In, ty: ValueType::Exec, default: None } }
fn exec_out(name: &str) -> Pin { Pin { name: name.into(), dir: PinDir::Out, ty: ValueType::Exec, default: None } }
fn data_in(name: &str, ty: ValueType) -> Pin { Pin { name: name.into(), dir: PinDir::In, ty, default: None } }
fn data_out(name: &str, ty: ValueType) -> Pin { Pin { name: name.into(), dir: PinDir::Out, ty, default: None } }

pub fn register_nodes(lib: &mut NodeLibrary) {
    lib.register(NodeDefinition {
        id: "flow.run".into(),
        purity: Purity::Impure,
        inputs: vec![exec_in(), data_in("raw_input", ValueType::String), data_in("state_json", ValueType::String)],
        outputs: vec![exec_out("exec"), data_out("result", ValueType::String)],
        runner: RunnerKind::Async(Arc::new(FlowRunner)),
    });
}

struct FlowRunner;

#[async_trait::async_trait]
impl AsyncNodeRunner for FlowRunner {
    async fn invoke(
        &self,
        inputs: &[Value],
        outputs: &mut [Value],
        ctx: &mut InvokeCtx<'_>,
    ) -> VsResult<ExecNext> {
        let _ = (inputs, ctx);
        outputs[1] = Value::String(r#"{"stages":["perception","memory","reasoning","execution"]}"#.into());
        Ok(ExecNext::Pin("exec".into()))
    }
}
