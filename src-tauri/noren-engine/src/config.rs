use std::path::PathBuf;

use crate::types::{default_theme, Config, InferenceMode, ProviderConfig};

const CONFIG_DIR_NAME: &str = ".noren";
const CONFIG_FILE_NAME: &str = "config.json";

/// Optional overrides that can be passed from CLI args or Tauri commands
#[derive(Debug, Default)]
pub struct ConfigOverrides {
    pub provider: Option<ProviderConfig>,
    pub profile_dir: Option<PathBuf>,
    pub server_url: Option<String>,
}

/// Load config with priority: overrides > env vars > config file > defaults
pub fn load_config(overrides: Option<ConfigOverrides>) -> Config {
    let defaults = Config::default();
    let file_config = load_file_config();
    let env_provider = load_env_provider();
    let overrides = overrides.unwrap_or_default();

    Config {
        provider: overrides
            .provider
            .or(env_provider)
            .or(file_config.provider)
            .unwrap_or(defaults.provider),
        profile_dir: overrides
            .profile_dir
            .or(file_config.profile_dir)
            .unwrap_or(defaults.profile_dir),
        server_url: overrides
            .server_url
            .or_else(|| std::env::var("NOREN_SERVER_URL").ok())
            .or(file_config.server_url)
            .or(defaults.server_url),
        inference_mode: file_config.inference_mode.unwrap_or(defaults.inference_mode),
        living_profile_enabled: file_config.living_profile_enabled.unwrap_or(false),
        hotkey: file_config.hotkey.unwrap_or(defaults.hotkey),
        extended_thinking: file_config.extended_thinking.unwrap_or(false),
        thinking_budget: file_config.thinking_budget.unwrap_or(10000),
        debug_mode: file_config.debug_mode.unwrap_or(false),
        last_seen_announcement_ts: file_config.last_seen_announcement_ts,
        theme: file_config.theme.unwrap_or_else(default_theme),
    }
}

// --- Private helpers ---

#[derive(Default)]
struct PartialConfig {
    provider: Option<ProviderConfig>,
    profile_dir: Option<PathBuf>,
    server_url: Option<String>,
    inference_mode: Option<InferenceMode>,
    living_profile_enabled: Option<bool>,
    hotkey: Option<String>,
    extended_thinking: Option<bool>,
    thinking_budget: Option<u32>,
    debug_mode: Option<bool>,
    last_seen_announcement_ts: Option<String>,
    theme: Option<String>,
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

    // Try new format first (nested provider object)
    let provider = json
        .get("provider")
        .and_then(|v| {
            if v.is_object() {
                serde_json::from_value::<ProviderConfig>(v.clone()).ok()
            } else if let Some(name) = v.as_str() {
                // Legacy format: provider was a string like "anthropic"
                ProviderConfig::preset_by_name(name)
            } else {
                None
            }
        });

    let inference_mode = json
        .get("inferenceMode")
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "noren_pro" => Some(InferenceMode::NorenPro),
            "byok" => Some(InferenceMode::Byok),
            _ => None,
        });

    PartialConfig {
        provider,
        profile_dir: json
            .get("profileDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
        server_url: json
            .get("serverUrl")
            .and_then(|v| v.as_str())
            .map(String::from),
        inference_mode,
        living_profile_enabled: json
            .get("livingProfileEnabled")
            .and_then(|v| v.as_bool()),
        hotkey: json
            .get("hotkey")
            .and_then(|v| v.as_str())
            .map(String::from),
        extended_thinking: json
            .get("extendedThinking")
            .and_then(|v| v.as_bool()),
        thinking_budget: json
            .get("thinkingBudget")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        debug_mode: json
            .get("debugMode")
            .and_then(|v| v.as_bool()),
        last_seen_announcement_ts: json
            .get("lastSeenAnnouncementTs")
            .and_then(|v| v.as_str())
            .map(String::from),
        theme: json
            .get("theme")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Load provider config from environment variables (legacy support)
fn load_env_provider() -> Option<ProviderConfig> {
    let provider_name = std::env::var("NOREN_PROVIDER").ok()?;
    let model = std::env::var("NOREN_EXTRACTION_MODEL").ok();

    let mut config = ProviderConfig::preset_by_name(&provider_name)?;
    if let Some(m) = model {
        config.model = m;
    }
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderType;

    #[test]
    fn defaults_are_sane() {
        // Test Config::default() directly to avoid depending on disk config
        let config = Config::default();
        assert_eq!(config.provider.provider_type, ProviderType::Anthropic);
        assert_eq!(config.provider.model, "claude-sonnet-4-6");
        assert!(config.profile_dir.to_string_lossy().contains(".noren/profiles"));
        assert!(!config.extended_thinking);
        assert_eq!(config.thinking_budget, 10000);
    }

    #[test]
    fn overrides_take_precedence() {
        let config = load_config(Some(ConfigOverrides {
            provider: Some(ProviderConfig::openai()),
            ..Default::default()
        }));
        assert_eq!(config.provider.provider_type, ProviderType::OpenaiCompatible);
        assert_eq!(config.provider.model, "gpt-4o");
    }

    #[test]
    fn preset_providers_exist() {
        assert!(ProviderConfig::preset_by_name("anthropic").is_some());
        assert!(ProviderConfig::preset_by_name("openai").is_some());
        assert!(ProviderConfig::preset_by_name("gemini").is_some());
        assert!(ProviderConfig::preset_by_name("ollama").is_some());
        assert!(ProviderConfig::preset_by_name("unknown").is_none());
    }

    #[test]
    fn ollama_does_not_require_key() {
        let config = ProviderConfig::ollama();
        assert!(!config.requires_key);
    }
}
