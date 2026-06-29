//! 错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VsError {
    #[error("validation error: {0}")]
    Validate(String),
    #[error("compile error: {0}")]
    Compile(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("type mismatch: expected {expected:?}, got {got:?} at {location}")]
    TypeMismatch {
        expected: crate::value::ValueType,
        got: crate::value::ValueType,
        location: String,
    },
    #[error("unknown node def: {0}")]
    UnknownDef(String),
    #[error("unknown node id: {0}")]
    UnknownNode(u32),
    #[error("cycle detected involving node {0}")]
    Cycle(u32),
    #[error("execution cancelled")]
    Cancelled,
    #[error("async runner invoked from synchronous Vm path")]
    AsyncRunnerInSyncVm,
}

pub type VsResult<T> = Result<T, VsError>;
