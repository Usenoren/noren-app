use std::path::PathBuf;

use crate::error::EngineError;
use crate::types::{Config, Provider};

const CONFIG_DIR_NAME: &str = ".noren";
const CONFIG_FILE_NAME: &str = "config.json";

/// Optional overrides that can be passed from CLI args or Tauri commands
#[derive(Debug, Default)]
pub struct ConfigOverrides {
    pub provider: Option<Provider>,
    pub model: Option<String>,
    pub profile_dir: Option<PathBuf>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub server_url: Option<String>,
}

/// Load config with priority: overrides > env vars > config file > defaults
pub fn load_config(overrides: Option<ConfigOverrides>) -> Config {
    let defaults = Config::default();
    let file_config = load_file_config();
    let env_config = load_env_config();
    let overrides = overrides.unwrap_or_default();

    Config {
        provider: overrides
            .provider
            .or(env_config.provider)
            .or(file_config.provider)
            .unwrap_or(defaults.provider),
        model: overrides
            .model
            .or(env_config.model)
            .or(file_config.model)
            .unwrap_or(defaults.model),
        profile_dir: overrides
            .profile_dir
            .or(file_config.profile_dir)
            .unwrap_or(defaults.profile_dir),
        anthropic_api_key: overrides
            .anthropic_api_key
            .or(env_config.anthropic_api_key)
            .or(file_config.anthropic_api_key),
        openai_api_key: overrides
            .openai_api_key
            .or(env_config.openai_api_key)
            .or(file_config.openai_api_key),
        gemini_api_key: overrides
            .gemini_api_key
            .or(env_config.gemini_api_key)
            .or(file_config.gemini_api_key),
        server_url: overrides
            .server_url
            .or(env_config.server_url)
            .or(file_config.server_url)
            .or(defaults.server_url),
    }
}

/// Get the API key for the configured provider
pub fn get_api_key(config: &Config) -> Result<String, EngineError> {
    match config.provider {
        Provider::Anthropic => config
            .anthropic_api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| EngineError::MissingApiKey("anthropic".to_string())),
        Provider::OpenAI => config
            .openai_api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| EngineError::MissingApiKey("openai".to_string())),
        Provider::Gemini => config
            .gemini_api_key
            .clone()
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .ok_or_else(|| EngineError::MissingApiKey("gemini".to_string())),
    }
}

// --- Private helpers ---

#[derive(Default)]
struct PartialConfig {
    provider: Option<Provider>,
    model: Option<String>,
    profile_dir: Option<PathBuf>,
    anthropic_api_key: Option<String>,
    openai_api_key: Option<String>,
    gemini_api_key: Option<String>,
    server_url: Option<String>,
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let new_dir = PathBuf::from(&home).join(CONFIG_DIR_NAME);

    // Migrate from legacy ~/.writewithme if it exists and ~/.noren doesn't
    if !new_dir.exists() {
        let legacy_dir = PathBuf::from(&home).join(".writewithme");
        if legacy_dir.exists() {
            if let Err(e) = copy_dir_recursive(&legacy_dir, &new_dir) {
                eprintln!("Warning: failed to migrate ~/.writewithme → ~/.noren: {}", e);
            } else {
                eprintln!("Migrated config from ~/.writewithme → ~/.noren");
            }
        }
    }

    new_dir
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn load_file_config() -> PartialConfig {
    let path = config_dir().join(CONFIG_FILE_NAME);
    if !path.exists() {
        return PartialConfig::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return PartialConfig::default(),
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return PartialConfig::default(),
    };

    PartialConfig {
        provider: json
            .get("provider")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok()),
        model: json
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from),
        profile_dir: json
            .get("profileDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
        anthropic_api_key: json
            .get("anthropicApiKey")
            .and_then(|v| v.as_str())
            .map(String::from),
        openai_api_key: json
            .get("openaiApiKey")
            .and_then(|v| v.as_str())
            .map(String::from),
        gemini_api_key: json
            .get("geminiApiKey")
            .and_then(|v| v.as_str())
            .map(String::from),
        server_url: json
            .get("serverUrl")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

fn load_env_config() -> PartialConfig {
    PartialConfig {
        provider: std::env::var("NOREN_PROVIDER")
            .ok()
            .and_then(|s| s.parse().ok()),
        model: std::env::var("NOREN_EXTRACTION_MODEL").ok(),
        profile_dir: None,
        anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
        gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
        server_url: std::env::var("NOREN_SERVER_URL").ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = load_config(None);
        assert_eq!(config.provider, Provider::Anthropic);
        assert_eq!(config.model, "claude-sonnet-4-20250514");
        assert!(config.profile_dir.to_string_lossy().contains(".noren/profiles"));
    }

    #[test]
    fn overrides_take_precedence() {
        let config = load_config(Some(ConfigOverrides {
            provider: Some(Provider::OpenAI),
            model: Some("gpt-4o".to_string()),
            ..Default::default()
        }));
        assert_eq!(config.provider, Provider::OpenAI);
        assert_eq!(config.model, "gpt-4o");
    }

    #[test]
    fn get_api_key_returns_error_when_missing() {
        let config = Config::default();
        let result = get_api_key(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("anthropic"));
    }

    #[test]
    fn get_api_key_from_config() {
        let mut config = Config::default();
        config.anthropic_api_key = Some("sk-test-123".to_string());
        let key = get_api_key(&config).unwrap();
        assert_eq!(key, "sk-test-123");
    }

    #[test]
    fn provider_from_str() {
        assert_eq!("anthropic".parse::<Provider>().unwrap(), Provider::Anthropic);
        assert_eq!("openai".parse::<Provider>().unwrap(), Provider::OpenAI);
        assert_eq!("gemini".parse::<Provider>().unwrap(), Provider::Gemini);
        assert!("unknown".parse::<Provider>().is_err());
    }
}
