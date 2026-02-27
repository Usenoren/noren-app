use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::LlmClient;
use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse, Role};

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        let system_message = messages.iter().find(|m| m.role == Role::System);
        let non_system: Vec<Content> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| Content {
                role: Some(match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "model".to_string(),
                    Role::System => unreachable!(),
                }),
                parts: vec![Part {
                    text: m.content.clone(),
                }],
            })
            .collect();

        let request = ApiRequest {
            contents: non_system,
            system_instruction: system_message.map(|m| Content {
                role: None,
                parts: vec![Part {
                    text: m.content.clone(),
                }],
            }),
            generation_config: Some(GenerationConfig {
                temperature: options.temperature,
                max_output_tokens: Some(options.max_tokens.unwrap_or(8192)),
            }),
        };

        let url = format!(
            "{}/{}:generateContent?key={}",
            API_BASE, self.model, self.api_key
        );

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(EngineError::Llm(format!(
                "Gemini API error ({}): {}",
                status, body
            )));
        }

        let api_resp: ApiResponse = resp.json().await?;

        let content = api_resp
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .unwrap_or_default();

        let usage = api_resp.usage_metadata.unwrap_or(UsageMetadata {
            prompt_token_count: 0,
            candidates_token_count: 0,
        });

        Ok(LlmResponse {
            content,
            input_tokens: usage.prompt_token_count,
            output_tokens: usage.candidates_token_count,
        })
    }

    fn provider(&self) -> &str {
        "gemini"
    }
}
