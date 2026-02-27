use async_trait::async_trait;

use crate::error::EngineError;

/// Result of an extraction run
pub struct ExtractionResult {
    pub core_identity: String,
    pub contexts: std::collections::HashMap<String, String>,
    pub quality_check: String,
}

/// Trait for extraction clients (server-side extraction is the moat)
#[async_trait]
pub trait ExtractionClient: Send + Sync {
    async fn extract(
        &self,
        samples: &str,
        format: &str,
    ) -> Result<ExtractionResult, EngineError>;
}

/// Stub client that returns an error directing users to the CLI.
/// Will be replaced with a real API client when the server is built.
pub struct StubExtractionClient;

#[async_trait]
impl ExtractionClient for StubExtractionClient {
    async fn extract(
        &self,
        _samples: &str,
        _format: &str,
    ) -> Result<ExtractionResult, EngineError> {
        Err(EngineError::Profile(
            "Extraction API not yet available. Use the CLI: `noren extract --samples your-writing.txt`"
                .to_string(),
        ))
    }
}
