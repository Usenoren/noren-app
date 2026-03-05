use crate::error::EngineError;
use crate::types::{Config, ProviderType};

use super::anthropic::AnthropicClient;
use super::openai_compatible::OpenAiCompatibleClient;
use super::LlmClient;

/// Create an LLM client based on the config's provider settings.
///
/// Routes to AnthropicClient for Anthropic's API format, or
/// OpenAiCompatibleClient for everything else.
pub fn create_llm_client(
    config: &Config,
    api_key: Option<String>,
) -> Result<Box<dyn LlmClient>, EngineError> {
    let provider = &config.provider;

    match provider.provider_type {
        ProviderType::Anthropic => {
            let key = api_key.ok_or_else(|| {
                EngineError::MissingApiKey(provider.name.clone())
            })?;
            Ok(Box::new(AnthropicClient::new(
                key,
                provider.model.clone(),
                provider.name.clone(),
            )))
        }
        ProviderType::OpenaiCompatible => {
            // For providers that don't require a key (e.g. Ollama), api_key can be None
            if provider.requires_key && api_key.is_none() {
                return Err(EngineError::MissingApiKey(provider.name.clone()));
            }
            Ok(Box::new(OpenAiCompatibleClient::new(
                api_key,
                provider.base_url.clone(),
                provider.model.clone(),
                provider.name.clone(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderConfig;

    #[test]
    fn router_creates_anthropic_client() {
        let config = Config {
            provider: ProviderConfig::anthropic(),
            ..Config::default()
        };
        let client = create_llm_client(&config, Some("test-key".to_string())).unwrap();
        assert_eq!(client.provider(), "anthropic");
    }

    #[test]
    fn router_creates_openai_compatible_client() {
        let config = Config {
            provider: ProviderConfig::openai(),
            ..Config::default()
        };
        let client = create_llm_client(&config, Some("test-key".to_string())).unwrap();
        assert_eq!(client.provider(), "openai");
    }

    #[test]
    fn router_allows_no_key_for_ollama() {
        let config = Config {
            provider: ProviderConfig::ollama(),
            ..Config::default()
        };
        let client = create_llm_client(&config, None).unwrap();
        assert_eq!(client.provider(), "ollama");
    }

    #[test]
    fn router_errors_without_api_key_for_anthropic() {
        let config = Config {
            provider: ProviderConfig::anthropic(),
            ..Config::default()
        };
        assert!(create_llm_client(&config, None).is_err());
    }

    #[test]
    fn router_errors_without_api_key_for_openai() {
        let config = Config {
            provider: ProviderConfig::openai(),
            ..Config::default()
        };
        assert!(create_llm_client(&config, None).is_err());
    }
}
