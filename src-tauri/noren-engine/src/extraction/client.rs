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

/// A format group for multi-format extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatGroup {
    pub format: String,
    pub samples: String,
}

/// Trait for extraction clients (server-side extraction is the moat)
#[async_trait]
pub trait ExtractionClient: Send + Sync {
    async fn extract(
        &mut self,
        samples: &str,
        format: &str,
    ) -> Result<ExtractionResult, EngineError>;

    async fn extract_multi(
        &mut self,
        format_groups: &[FormatGroup],
    ) -> Result<ExtractionResult, EngineError>;
}

/// Server-side extraction client that calls the Noren API.
pub struct ServerExtractionClient {
    server_url: String,
    auth_token: String,
    http: reqwest::Client,
    on_progress: Option<ProgressCallback>,
    refresh_token: Option<String>,
    on_tokens_refreshed: Option<Box<dyn Fn(String, String) + Send + Sync>>,
    calibration: Option<serde_json::Value>,
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
            refresh_token: None,
            on_tokens_refreshed: None,
            calibration: None,
        }
    }

    pub fn with_progress(mut self, callback: ProgressCallback) -> Self {
        self.on_progress = Some(callback);
        self
    }

    pub fn with_calibration(mut self, calibration: serde_json::Value) -> Self {
        self.calibration = Some(calibration);
        self
    }

    /// Enable automatic token refresh on 401 responses.
    pub fn with_token_refresh(
        mut self,
        refresh_token: String,
        on_refreshed: impl Fn(String, String) + Send + Sync + 'static,
    ) -> Self {
        self.refresh_token = Some(refresh_token);
        self.on_tokens_refreshed = Some(Box::new(on_refreshed));
        self
    }

    /// Attempt to refresh the access token. Returns new token on success.
    async fn try_refresh(&mut self) -> Option<String> {
        let refresh = self.refresh_token.as_deref()?;

        let resp = self
            .http
            .post(format!("{}/v1/auth/refresh", self.server_url))
            .json(&serde_json::json!({ "refresh_token": refresh }))
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let data: serde_json::Value = resp.json().await.ok()?;
        let new_access = data["access_token"].as_str()?.to_string();
        let new_refresh = data["refresh_token"].as_str()?.to_string();

        if let Some(ref cb) = self.on_tokens_refreshed {
            cb(new_access.clone(), new_refresh.clone());
        }

        // Update stored tokens for subsequent requests in this session
        self.auth_token = new_access.clone();
        self.refresh_token = Some(new_refresh);

        Some(new_access)
    }

    /// Send an authenticated GET request, retrying once on 401 with token refresh.
    async fn authed_get(&mut self, url: &str) -> Result<reqwest::Response, EngineError> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.auth_token)
            .send()
            .await?;

        if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                let retry = self
                    .http
                    .get(url)
                    .bearer_auth(&new_token)
                    .send()
                    .await?;
                return Ok(retry);
            }
            return Err(EngineError::Llm("Session expired. Please sign in again.".to_string()));
        }

        Ok(resp)
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
        &mut self,
        samples: &str,
        format: &str,
    ) -> Result<ExtractionResult, EngineError> {
        let mut payload = serde_json::json!({
            "samples": samples,
            "format": format,
        });
        if let Some(ref cal) = self.calibration {
            payload["calibration"] = cal.clone();
        }
        self.run_extraction_job(payload).await
    }

    async fn extract_multi(
        &mut self,
        format_groups: &[FormatGroup],
    ) -> Result<ExtractionResult, EngineError> {
        let groups: Vec<_> = format_groups
            .iter()
            .map(|fg| serde_json::json!({"format": fg.format, "samples": fg.samples}))
            .collect();
        let payload = serde_json::json!({
            "format_groups": groups,
        });
        self.run_extraction_job(payload).await
    }
}

impl ServerExtractionClient {
    /// Shared extraction job logic: start, poll, get result.
    async fn run_extraction_job(
        &mut self,
        payload: serde_json::Value,
    ) -> Result<ExtractionResult, EngineError> {
        // Step 1: Start the extraction job
        let start_url = format!("{}/v1/extract", self.server_url);
        let resp = self
            .http
            .post(&start_url)
            .bearer_auth(&self.auth_token)
            .json(&payload)
            .send()
            .await?;

        // Handle 401 on start
        let resp = if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                self.http
                    .post(&start_url)
                    .bearer_auth(&new_token)
                    .json(&payload)
                    .send()
                    .await?
            } else {
                return Err(EngineError::Llm("Session expired. Please sign in again.".to_string()));
            }
        } else {
            resp
        };

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

        // Step 2: Poll for completion (uses authed_get for automatic token refresh)
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let poll_url = format!("{}/v1/extract/{}", self.server_url, job_id);
            let resp = self.authed_get(&poll_url).await?;

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
        let result_url = format!("{}/v1/extract/{}/result", self.server_url, job_id);
        let resp = self.authed_get(&result_url).await?;

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
