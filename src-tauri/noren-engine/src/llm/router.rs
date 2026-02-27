use crate::config::get_api_key;
use crate::error::EngineError;
use crate::types::{Config, Provider};

use super::anthropic::AnthropicClient;
use super::gemini::GeminiClient;
use super::openai::OpenAiClient;
use super::LlmClient;

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";
const GEMINI_DEFAULT_MODEL: &str = "gemini-2.5-flash";

/// Create an LLM client based on the config.
///
/// If the model is the default Anthropic model but a non-Anthropic provider is selected,
/// the model is overridden to that provider's default.
pub fn create_llm_client(config: &Config) -> Result<Box<dyn LlmClient>, EngineError> {
    let api_key = get_api_key(config)?;

    match config.provider {
        Provider::Anthropic => Ok(Box::new(AnthropicClient::new(api_key, config.model.clone()))),
        Provider::OpenAI => {
            let model = if config.model == DEFAULT_ANTHROPIC_MODEL {
                OPENAI_DEFAULT_MODEL.to_string()
            } else {
                config.model.clone()
            };
            Ok(Box::new(OpenAiClient::new(api_key, model)))
        }
        Provider::Gemini => {
            let model = if config.model == DEFAULT_ANTHROPIC_MODEL {
                GEMINI_DEFAULT_MODEL.to_string()
            } else {
                config.model.clone()
            };
            Ok(Box::new(GeminiClient::new(api_key, model)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_overrides_model_for_openai() {
        let config = Config {
            provider: Provider::OpenAI,
            model: DEFAULT_ANTHROPIC_MODEL.to_string(),
            openai_api_key: Some("test-key".to_string()),
            ..Config::default()
        };
        let client = create_llm_client(&config).unwrap();
        assert_eq!(client.provider(), "openai");
    }

    #[test]
    fn router_overrides_model_for_gemini() {
        let config = Config {
            provider: Provider::Gemini,
            model: DEFAULT_ANTHROPIC_MODEL.to_string(),
            gemini_api_key: Some("test-key".to_string()),
            ..Config::default()
        };
        let client = create_llm_client(&config).unwrap();
        assert_eq!(client.provider(), "gemini");
    }

    #[test]
    fn router_errors_without_api_key() {
        let config = Config {
            provider: Provider::Anthropic,
            ..Config::default()
        };
        assert!(create_llm_client(&config).is_err());
    }
}
