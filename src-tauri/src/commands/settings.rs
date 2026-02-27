use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{keychain, AppState};

#[derive(Serialize)]
pub struct SettingsInfo {
    pub provider: noren_engine::ProviderConfig,
    pub has_key: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsInfo {
    let config = state.config.lock().unwrap();
    let provider = &config.provider;

    let has_key = if !provider.requires_key {
        true // Local providers like Ollama don't need a key
    } else {
        keychain::get_api_key(&provider.keychain_id()).is_some()
    };

    SettingsInfo {
        provider: provider.clone(),
        has_key,
    }
}

#[derive(Deserialize)]
pub struct SetProviderArgs {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: Option<String>,
    #[serde(rename = "baseUrl")]
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "requiresKey")]
    pub requires_key: Option<bool>,
}

#[tauri::command]
pub fn set_provider(
    state: State<'_, AppState>,
    provider: SetProviderArgs,
) -> Result<(), String> {
    let provider_config = if let Some(preset) = noren_engine::ProviderConfig::preset_by_name(&provider.name) {
        // Use preset, but allow model override
        let mut config = preset;
        if let Some(m) = provider.model {
            config.model = m;
        }
        if let Some(url) = provider.base_url {
            config.base_url = url;
        }
        config
    } else {
        // Custom provider — all fields required
        let provider_type = match provider.provider_type.as_deref() {
            Some("anthropic") => noren_engine::ProviderType::Anthropic,
            _ => noren_engine::ProviderType::OpenaiCompatible,
        };
        noren_engine::ProviderConfig {
            name: provider.name,
            provider_type,
            base_url: provider.base_url.ok_or("Base URL required for custom provider")?,
            model: provider.model.ok_or("Model required for custom provider")?,
            requires_key: provider.requires_key.unwrap_or(true),
        }
    };

    let mut config = state.config.lock().unwrap();
    config.provider = provider_config;
    save_config_file(&config);

    Ok(())
}

#[tauri::command]
pub fn update_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.provider.model = model;
    save_config_file(&config);
    Ok(())
}

#[tauri::command]
pub fn update_base_url(
    state: State<'_, AppState>,
    base_url: String,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.provider.base_url = base_url;
    save_config_file(&config);
    Ok(())
}

#[tauri::command]
pub fn save_api_key(
    state: State<'_, AppState>,
    key: String,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let keychain_id = config.provider.keychain_id();
    keychain::store_api_key(&keychain_id, &key)
}

#[tauri::command]
pub fn remove_api_key(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let keychain_id = config.provider.keychain_id();
    keychain::delete_api_key(&keychain_id)
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    key: Option<String>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let provider = &config.provider;

    // Resolve API key: provided key > keychain > none (for local providers)
    let api_key = if provider.requires_key {
        let k = key
            .or_else(|| keychain::get_api_key(&provider.keychain_id()));
        if k.is_none() {
            return Err(format!("No API key for {}", provider.name));
        }
        k
    } else {
        None
    };

    let client = noren_engine::create_llm_client(&config, api_key)
        .map_err(|e| e.to_string())?;

    let messages = vec![noren_engine::LlmMessage {
        role: noren_engine::Role::User,
        content: "Say 'ok'".to_string(),
    }];
    let options = noren_engine::LlmOptions {
        temperature: Some(0.0),
        max_tokens: Some(5),
    };

    client
        .complete(&messages, &options)
        .await
        .map(|r| r.content)
        .map_err(|e| e.to_string())
}

/// Persist config to ~/.noren/config.json
fn save_config_file(config: &noren_engine::Config) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let config_dir = std::path::PathBuf::from(home).join(".noren");
    let _ = std::fs::create_dir_all(&config_dir);

    let json = serde_json::json!({
        "provider": config.provider,
        "profileDir": config.profile_dir.to_string_lossy(),
    });

    let _ = std::fs::write(
        config_dir.join("config.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
}
