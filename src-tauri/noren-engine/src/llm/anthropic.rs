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
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
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

        let request = ApiRequest {
            model: self.model.clone(),
            max_tokens: options.max_tokens.unwrap_or(8192),
            temperature: options.temperature,
            system: system_message.map(|m| m.content.clone()),
            messages: non_system,
        };

        let resp = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

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
