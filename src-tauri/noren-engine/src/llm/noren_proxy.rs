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
}

impl NorenProxyClient {
    pub fn new(server_url: String, auth_token: String, format: String) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            auth_token,
            format,
            http: reqwest::Client::new(),
        }
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
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body: String = resp.text().await.unwrap_or_default();

            // Try to parse structured error
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
