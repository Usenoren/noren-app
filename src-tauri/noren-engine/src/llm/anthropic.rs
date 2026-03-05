use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::LlmClient;
use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse, Role};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
    provider_name: String,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String, provider_name: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            provider_name,
        }
    }
}

// --- Request/Response types ---

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ApiThinking>,
}

#[derive(Serialize)]
struct ApiThinking {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: u32,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    usage: Usage,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        let system_message = messages.iter().find(|m| m.role == Role::System);
        let non_system: Vec<ApiMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| ApiMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect();

        let (thinking_field, max_tokens) = if let Some(ref tc) = options.thinking {
            (
                Some(ApiThinking {
                    thinking_type: "enabled".to_string(),
                    budget_tokens: tc.budget_tokens,
                }),
                tc.budget_tokens + options.max_tokens.unwrap_or(4096),
            )
        } else {
            (None, options.max_tokens.unwrap_or(8192))
        };

        let request = ApiRequest {
            model: self.model.clone(),
            max_tokens,
            temperature: if thinking_field.is_some() { None } else { options.temperature },
            system: system_message.map(|m| m.content.clone()),
            messages: non_system,
            thinking: thinking_field,
        };

        let mut req_builder = self
            .client
            .post(API_URL)
            .header("content-type", "application/json")
            .header("anthropic-version", API_VERSION);

        if self.provider_name == "claude-token" {
            req_builder = req_builder
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("anthropic-beta", "oauth-2025-04-20");
        } else {
            req_builder = req_builder.header("x-api-key", &self.api_key);
        }

        let resp = req_builder.json(&request).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                return Err(EngineError::Llm(format!(
                    "Anthropic API error ({}): {}",
                    status, api_err.error.message
                )));
            }
            return Err(EngineError::Llm(format!(
                "Anthropic API error ({}): {}",
                status, body
            )));
        }

        let api_resp: ApiResponse = resp.json().await?;

        let content = api_resp
            .content
            .iter()
            .filter(|b| b.block_type == "text")
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse {
            content,
            input_tokens: api_resp.usage.input_tokens,
            output_tokens: api_resp.usage.output_tokens,
        })
    }

    fn provider(&self) -> &str {
        "anthropic"
    }
}
