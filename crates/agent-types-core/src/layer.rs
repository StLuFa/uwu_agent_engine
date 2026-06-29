//! Layer<I,O> generic pipeline trait

use async_trait::async_trait;

/// 通用管道层：Input → Output
#[async_trait]
pub trait Layer<I, O>: Send + Sync
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    async fn process(&self, input: I) -> O;
}
