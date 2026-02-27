pub mod anthropic;
pub mod noren_proxy;
pub mod openai_compatible;
pub mod router;

use async_trait::async_trait;

use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse};

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError>;

    fn provider(&self) -> &str;
}
