//! Noren Pro proxy client — sends generation requests to the Noren server
//! instead of calling LLM providers directly. No API key needed on client.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse, Role};

use super::LlmClient;

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct GenerateRequest {
    messages: Vec<ChatMessage>,
    format: String,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_title: Option<String>,
}

/// Server-composed request — no messages, no profile content.
/// Server loads profile and composes prompt from its side.
#[derive(Serialize)]
struct ServerComposedRequest {
    prompt: String,
    format: String,
    level: String,
    /// Pipeline selection: "internalized" (default) or "light"
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    /// Generation mode: "generate" (default) or "adapt"
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attachments: Option<Vec<String>>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GenerateResponse {
    content: String,
    input_tokens: u64,
    output_tokens: u64,
    #[allow(dead_code)]
    model: String,
}

#[derive(Deserialize)]
struct UsageResponse {
    pub tokens_used: u64,
    pub tokens_limit: u64,
    pub requests_this_month: u64,
}

#[derive(Deserialize)]
struct ErrorDetail {
    detail: String,
}

pub struct NorenProxyClient {
    server_url: String,
    auth_token: String,
    format: String,
    http: reqwest::Client,
    refresh_token: Option<String>,
    on_tokens_refreshed: Option<Box<dyn Fn(String, String) + Send + Sync>>,
}

impl NorenProxyClient {
    pub fn new(server_url: String, auth_token: String, format: String) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            auth_token,
            format,
            http: reqwest::Client::builder()
                .pool_max_idle_per_host(0)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            refresh_token: None,
            on_tokens_refreshed: None,
        }
    }

    /// Enable automatic token refresh on 401 responses.
    /// `refresh_token`: the current refresh token.
    /// `on_refreshed`: callback invoked with (new_access, new_refresh) after a successful refresh.
    pub fn with_token_refresh(
        mut self,
        refresh_token: String,
        on_refreshed: impl Fn(String, String) + Send + Sync + 'static,
    ) -> Self {
        self.refresh_token = Some(refresh_token);
        self.on_tokens_refreshed = Some(Box::new(on_refreshed));
        self
    }

    /// Attempt to refresh the access token using the stored refresh token.
    /// Returns the new access token on success.
    async fn try_refresh(&self) -> Option<String> {
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
            cb(new_access.clone(), new_refresh);
        }

        Some(new_access)
    }

    /// Generate text with server-side prompt composition.
    ///
    /// The server loads the user's profile and composes the system prompt
    /// using the proprietary enforcement template. Client never sees
    /// profile content or real prompt.
    pub async fn generate_server_composed(
        &self,
        prompt: &str,
        format: &str,
        level: &str,
        pipeline: Option<&str>,
        generation_mode: Option<&str>,
        context: Option<&str>,
        attachments: Option<&[String]>,
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        let url = format!("{}/v1/generate/", self.server_url);

        let req = ServerComposedRequest {
            prompt: prompt.to_string(),
            format: format.to_string(),
            level: level.to_string(),
            mode: pipeline.map(|s| s.to_string()),
            generation_mode: generation_mode.map(|s| s.to_string()),
            context: context.map(|s| s.to_string()),
            attachments: attachments.map(|a| a.to_vec()),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        // Handle 401 with token refresh retry
        if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                let retry = self
                    .http
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", new_token))
                    .json(&req)
                    .send()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?;

                if retry.status().is_success() {
                    let gen: GenerateResponse = retry
                        .json::<GenerateResponse>()
                        .await
                        .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;
                    return Ok(LlmResponse {
                        content: gen.content,
                        input_tokens: gen.input_tokens,
                        output_tokens: gen.output_tokens,
                    });
                }
            }
            return Err(EngineError::Network(
                "Session expired. Please sign in again.".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body: String = resp.text().await.unwrap_or_default();

            if let Ok(err) = serde_json::from_str::<ErrorDetail>(&body) {
                return Err(EngineError::Network(err.detail));
            }
            return Err(EngineError::Network(format!(
                "Server error ({}): {}",
                status, body
            )));
        }

        let gen: GenerateResponse = resp
            .json::<GenerateResponse>()
            .await
            .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;

        Ok(LlmResponse {
            content: gen.content,
            input_tokens: gen.input_tokens,
            output_tokens: gen.output_tokens,
        })
    }

    /// Stream generation with server-side prompt composition.
    ///
    /// Returns a reqwest Response with `text/event-stream` body.
    /// Caller is responsible for reading SSE lines from the body.
    pub async fn generate_server_composed_stream(
        &self,
        prompt: &str,
        format: &str,
        level: &str,
        pipeline: Option<&str>,
        generation_mode: Option<&str>,
        context: Option<&str>,
        attachments: Option<&[String]>,
        options: &LlmOptions,
    ) -> Result<reqwest::Response, EngineError> {
        let url = format!("{}/v1/generate/", self.server_url);

        #[derive(Serialize)]
        struct StreamRequest {
            prompt: String,
            format: String,
            level: String,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            mode: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            generation_mode: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            context: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            attachments: Option<Vec<String>>,
            temperature: Option<f64>,
            max_tokens: Option<u32>,
        }

        let req = StreamRequest {
            prompt: prompt.to_string(),
            format: format.to_string(),
            level: level.to_string(),
            stream: true,
            mode: pipeline.map(|s| s.to_string()),
            generation_mode: generation_mode.map(|s| s.to_string()),
            context: context.map(|s| s.to_string()),
            attachments: attachments.map(|a| a.to_vec()),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
        };

        let mut auth = format!("Bearer {}", self.auth_token);

        let resp = self
            .http
            .post(&url)
            .header("Authorization", &auth)
            .json(&req)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        // Handle 401 with token refresh
        if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                auth = format!("Bearer {}", new_token);
                let retry = self
                    .http
                    .post(&url)
                    .header("Authorization", &auth)
                    .json(&req)
                    .send()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?;
                if retry.status().is_success() {
                    return Ok(retry);
                }
            }
            return Err(EngineError::Network(
                "Session expired. Please sign in again.".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<ErrorDetail>(&body) {
                return Err(EngineError::Network(err.detail));
            }
            return Err(EngineError::Network(format!("Server error ({}): {}", status, body)));
        }

        Ok(resp)
    }

    /// Fetch current usage from server.
    pub async fn get_usage(&self) -> Result<(u64, u64, u64), EngineError> {
        let url = format!("{}/v1/generate/usage", self.server_url);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        // Handle 401 with token refresh retry
        if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                let retry = self
                    .http
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", new_token))
                    .send()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?;

                if retry.status().is_success() {
                    let usage: UsageResponse = retry
                        .json::<UsageResponse>()
                        .await
                        .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;
                    return Ok((usage.tokens_used, usage.tokens_limit, usage.requests_this_month));
                }
            }
            return Err(EngineError::Network(
                "Session expired. Please sign in again.".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body: String = resp.text().await.unwrap_or_default();
            return Err(EngineError::Network(format!(
                "Usage check failed ({}): {}",
                status, body
            )));
        }

        let usage: UsageResponse = resp
            .json::<UsageResponse>()
            .await
            .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;

        Ok((
            usage.tokens_used,
            usage.tokens_limit,
            usage.requests_this_month,
        ))
    }
}

#[async_trait]
impl LlmClient for NorenProxyClient {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        let url = format!("{}/v1/generate/", self.server_url);

        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|m| ChatMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let req = GenerateRequest {
            messages: chat_messages,
            format: self.format.clone(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            chat_id: options.chat_id.clone(),
            chat_title: options.chat_title.clone(),
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        // Handle 401 with token refresh retry
        if resp.status().as_u16() == 401 {
            if let Some(new_token) = self.try_refresh().await {
                let retry = self
                    .http
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", new_token))
                    .json(&req)
                    .send()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?;

                if retry.status().is_success() {
                    let gen: GenerateResponse = retry
                        .json::<GenerateResponse>()
                        .await
                        .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;
                    return Ok(LlmResponse {
                        content: gen.content,
                        input_tokens: gen.input_tokens,
                        output_tokens: gen.output_tokens,
                    });
                }
            }
            return Err(EngineError::Network(
                "Session expired. Please sign in again.".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body: String = resp.text().await.unwrap_or_default();

            if let Ok(err) = serde_json::from_str::<ErrorDetail>(&body) {
                return Err(EngineError::Network(err.detail));
            }
            return Err(EngineError::Network(format!(
                "Server error ({}): {}",
                status, body
            )));
        }

        let gen: GenerateResponse = resp
            .json::<GenerateResponse>()
            .await
            .map_err(|e: reqwest::Error| EngineError::Network(e.to_string()))?;

        Ok(LlmResponse {
            content: gen.content,
            input_tokens: gen.input_tokens,
            output_tokens: gen.output_tokens,
        })
    }

    fn provider(&self) -> &str {
        "noren-pro"
    }
}
