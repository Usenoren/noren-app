use serde::Serialize;
use tauri::State;

use crate::{keychain, AppState};

#[derive(Serialize)]
pub struct SettingsInfo {
    pub provider: String,
    pub model: String,
    pub has_anthropic_key: bool,
    pub has_openai_key: bool,
    pub has_gemini_key: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsInfo {
    let config = state.config.lock().unwrap();
    SettingsInfo {
        provider: config.provider.to_string(),
        model: config.model.clone(),
        has_anthropic_key: keychain::get_api_key("anthropic").is_some()
            || config.anthropic_api_key.is_some(),
        has_openai_key: keychain::get_api_key("openai").is_some()
            || config.openai_api_key.is_some(),
        has_gemini_key: keychain::get_api_key("gemini").is_some()
            || config.gemini_api_key.is_some(),
    }
}

#[tauri::command]
pub fn save_api_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<(), String> {
    // Store in Keychain
    keychain::store_api_key(&provider, &key)?;

    // Update in-memory config so it's available immediately
    let mut config = state.config.lock().unwrap();
    match provider.as_str() {
        "anthropic" => config.anthropic_api_key = Some(key),
        "openai" => config.openai_api_key = Some(key),
        "gemini" => config.gemini_api_key = Some(key),
        _ => return Err(format!("Unknown provider: {}", provider)),
    }

    Ok(())
}

#[tauri::command]
pub fn remove_api_key(
    state: State<'_, AppState>,
    provider: String,
) -> Result<(), String> {
    keychain::delete_api_key(&provider)?;

    // Clear from in-memory config
    let mut config = state.config.lock().unwrap();
    match provider.as_str() {
        "anthropic" => config.anthropic_api_key = None,
        "openai" => config.openai_api_key = None,
        "gemini" => config.gemini_api_key = None,
        _ => return Err(format!("Unknown provider: {}", provider)),
    }

    Ok(())
}

#[tauri::command]
pub fn update_provider(
    state: State<'_, AppState>,
    provider: String,
) -> Result<(), String> {
    let provider_enum: noren_engine::Provider = provider
        .parse()
        .map_err(|e: String| e)?;

    let mut config = state.config.lock().unwrap();
    config.provider = provider_enum;

    // Persist to config file
    save_config_file(&config);

    Ok(())
}

#[tauri::command]
pub fn update_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.model = model;

    save_config_file(&config);

    Ok(())
}

#[tauri::command]
pub async fn test_api_key(
    provider: String,
    key: String,
    model: Option<String>,
) -> Result<String, String> {
    let provider_enum: noren_engine::Provider = provider
        .parse()
        .map_err(|e: String| e)?;

    let mut config = noren_engine::Config::default();
    config.provider = provider_enum;
    if let Some(m) = model {
        config.model = m;
    }

    match config.provider {
        noren_engine::Provider::Anthropic => config.anthropic_api_key = Some(key),
        noren_engine::Provider::OpenAI => config.openai_api_key = Some(key),
        noren_engine::Provider::Gemini => config.gemini_api_key = Some(key),
    }

    let client = noren_engine::create_llm_client(&config).map_err(|e| e.to_string())?;
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

    // Only persist provider and model (not API keys — those go to Keychain)
    let json = serde_json::json!({
        "provider": config.provider.to_string(),
        "model": &config.model,
        "profileDir": config.profile_dir.to_string_lossy(),
    });

    let _ = std::fs::write(
        config_dir.join("config.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
}
