use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::LlmClient;
use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse, Role};

/// Universal client for any provider that exposes an OpenAI-compatible
/// chat completions endpoint (OpenAI, Ollama, Groq, Together, Mistral,
/// Fireworks, OpenRouter, Gemini, LM Studio, vLLM, etc.).
pub struct OpenAiCompatibleClient {
    client: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    provider_name: String,
}

impl OpenAiCompatibleClient {
    pub fn new(
        api_key: Option<String>,
        base_url: String,
        model: String,
        provider_name: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            provider_name,
        }
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    messages: Vec<ApiMessage>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        let api_messages: Vec<ApiMessage> = messages
            .iter()
            .map(|m| ApiMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = ApiRequest {
            model: self.model.clone(),
            temperature: options.temperature,
            max_tokens: Some(options.max_tokens.unwrap_or(8192)),
            messages: api_messages,
        };

        let url = format!("{}/chat/completions", self.base_url);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.json(&request).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!(
                "{} API error ({}): {}",
                self.provider_name, status, body
            )));
        }

        let api_resp: ApiResponse = resp.json().await?;

        let content = api_resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = api_resp.usage.unwrap_or(ApiUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        });

        Ok(LlmResponse {
            content,
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        })
    }

    fn provider(&self) -> &str {
        &self.provider_name
    }
}
