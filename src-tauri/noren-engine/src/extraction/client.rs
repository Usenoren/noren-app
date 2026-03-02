use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;

/// Result of an extraction run
pub struct ExtractionResult {
    pub core_identity: String,
    pub contexts: std::collections::HashMap<String, String>,
    pub quality_check: String,
    /// When true, the profile is stored server-side — don't save locally.
    pub stored_server_side: bool,
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(ExtractionProgress) + Send + Sync>;

/// Extraction job progress info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionProgress {
    pub status: String,
    pub progress: u32,
    pub error: Option<String>,
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

/// Server-side extraction client that calls the Noren API.
pub struct ServerExtractionClient {
    server_url: String,
    auth_token: String,
    http: reqwest::Client,
    on_progress: Option<ProgressCallback>,
}

#[derive(Deserialize)]
struct StartJobResponse {
    job_id: String,
    status: String,
    progress: u32,
    #[allow(dead_code)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct JobStatusResponse {
    #[allow(dead_code)]
    job_id: String,
    status: String,
    progress: u32,
    error: Option<String>,
}

#[derive(Deserialize)]
struct JobResultResponse {
    #[allow(dead_code)]
    job_id: String,
    #[allow(dead_code)]
    status: String,
    core_identity: Option<String>,
    context: Option<std::collections::HashMap<String, String>>,
    quality_report: Option<String>,
    #[serde(default)]
    stored_server_side: bool,
}

#[derive(Deserialize)]
struct AuthResponse {
    access_token: String,
    #[allow(dead_code)]
    refresh_token: String,
    #[allow(dead_code)]
    token_type: String,
}

impl ServerExtractionClient {
    pub fn new(server_url: String, auth_token: String) -> Self {
        Self {
            server_url,
            auth_token,
            http: reqwest::Client::new(),
            on_progress: None,
        }
    }

    pub fn with_progress(mut self, callback: ProgressCallback) -> Self {
        self.on_progress = Some(callback);
        self
    }

    /// Register a new account and get auth token
    pub async fn register(
        server_url: &str,
        email: &str,
        password: &str,
    ) -> Result<String, EngineError> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/auth/register", server_url))
            .json(&serde_json::json!({
                "email": email,
                "password": password,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!("Registration failed: {}", text)));
        }

        let auth: AuthResponse = resp.json().await?;
        Ok(auth.access_token)
    }

    /// Login and get auth token
    pub async fn login(
        server_url: &str,
        email: &str,
        password: &str,
    ) -> Result<String, EngineError> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/auth/login", server_url))
            .json(&serde_json::json!({
                "email": email,
                "password": password,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!("Login failed: {}", text)));
        }

        let auth: AuthResponse = resp.json().await?;
        Ok(auth.access_token)
    }

    fn emit_progress(&self, progress: ExtractionProgress) {
        if let Some(ref cb) = self.on_progress {
            cb(progress);
        }
    }
}

#[async_trait]
impl ExtractionClient for ServerExtractionClient {
    async fn extract(
        &self,
        samples: &str,
        format: &str,
    ) -> Result<ExtractionResult, EngineError> {
        // Step 1: Start the extraction job
        let resp = self
            .http
            .post(format!("{}/v1/extract", self.server_url))
            .bearer_auth(&self.auth_token)
            .json(&serde_json::json!({
                "samples": samples,
                "format": format,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!("Failed to start extraction: {}", text)));
        }

        let job: StartJobResponse = resp.json().await?;
        let job_id = job.job_id;

        self.emit_progress(ExtractionProgress {
            status: job.status,
            progress: job.progress,
            error: None,
        });

        // Step 2: Poll for completion
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let resp = self
                .http
                .get(format!("{}/v1/extract/{}", self.server_url, job_id))
                .bearer_auth(&self.auth_token)
                .send()
                .await?;

            if !resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(EngineError::Llm(format!("Failed to poll job: {}", text)));
            }

            let status: JobStatusResponse = resp.json().await?;

            self.emit_progress(ExtractionProgress {
                status: status.status.clone(),
                progress: status.progress,
                error: status.error.clone(),
            });

            match status.status.as_str() {
                "completed" => break,
                "failed" => {
                    return Err(EngineError::Llm(format!(
                        "Extraction failed: {}",
                        status.error.unwrap_or_else(|| "Unknown error".to_string())
                    )));
                }
                _ => continue,
            }
        }

        // Step 3: Get the result
        let resp = self
            .http
            .get(format!("{}/v1/extract/{}/result", self.server_url, job_id))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!("Failed to get result: {}", text)));
        }

        let result: JobResultResponse = resp.json().await?;

        Ok(ExtractionResult {
            core_identity: result.core_identity.unwrap_or_default(),
            contexts: result.context.unwrap_or_default(),
            quality_check: result.quality_report.unwrap_or_default(),
            stored_server_side: result.stored_server_side,
        })
    }
}
