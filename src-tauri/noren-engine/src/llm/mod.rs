pub mod anthropic;
pub mod claude_code_proxy;
pub mod noren_proxy;
pub mod openai_compatible;
pub mod router;

use async_trait::async_trait;

use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse};

pub type StreamCallback = Box<dyn Fn(&str) + Send + Sync>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError>;

    async fn stream_complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
        on_chunk: StreamCallback,
    ) -> Result<LlmResponse, EngineError> {
        let response = self.complete(messages, options).await?;
        on_chunk(&response.content);
        Ok(response)
    }

    fn provider(&self) -> &str;
}
