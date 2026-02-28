use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{keychain, AppState};

#[derive(Serialize)]
pub struct SettingsInfo {
    pub provider: noren_engine::ProviderConfig,
    pub has_key: bool,
    pub inference_mode: String,
    pub noren_pro_logged_in: bool,
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

    let mode = match config.inference_mode {
        noren_engine::InferenceMode::NorenPro => "noren_pro",
        noren_engine::InferenceMode::Byok => "byok",
    };

    SettingsInfo {
        provider: provider.clone(),
        has_key,
        inference_mode: mode.to_string(),
        noren_pro_logged_in: keychain::get_api_key("noren-pro-token").is_some(),
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

// --- Noren Pro auth ---

#[derive(Serialize)]
pub struct NorenProStatus {
    pub logged_in: bool,
    pub email: Option<String>,
    pub inference_mode: String,
    pub tokens_used: Option<u64>,
    pub tokens_limit: Option<u64>,
    pub requests_this_month: Option<u64>,
}

#[tauri::command]
pub fn get_noren_pro_status(state: State<'_, AppState>) -> NorenProStatus {
    let config = state.config.lock().unwrap();
    let has_token = keychain::get_api_key("noren-pro-token").is_some();
    let email = keychain::get_api_key("noren-pro-email");

    let mode = match config.inference_mode {
        noren_engine::InferenceMode::NorenPro => "noren_pro",
        noren_engine::InferenceMode::Byok => "byok",
    };

    NorenProStatus {
        logged_in: has_token,
        email,
        inference_mode: mode.to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
    }
}

#[tauri::command]
pub async fn noren_pro_login(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<NorenProStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    let client = reqwest::Client::new();

    // Try login
    let resp: reqwest::Response = client
        .post(format!("{}/v1/auth/login", server_url))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Login failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = data["access_token"]
        .as_str()
        .ok_or("No access token in response")?;

    // Store token and email in keychain
    keychain::store_api_key("noren-pro-token", token)?;
    keychain::store_api_key("noren-pro-email", &email)?;

    Ok(NorenProStatus {
        logged_in: true,
        email: Some(email),
        inference_mode: "noren_pro".to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
    })
}

#[tauri::command]
pub async fn noren_pro_signup(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<NorenProStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    let client = reqwest::Client::new();

    // Register
    let resp: reqwest::Response = client
        .post(format!("{}/v1/auth/register", server_url))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Signup failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = data["access_token"]
        .as_str()
        .ok_or("No access token in response")?;

    // Store token and email in keychain
    keychain::store_api_key("noren-pro-token", token)?;
    keychain::store_api_key("noren-pro-email", &email)?;

    Ok(NorenProStatus {
        logged_in: true,
        email: Some(email),
        inference_mode: "noren_pro".to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
    })
}

#[tauri::command]
pub fn noren_pro_logout() -> Result<(), String> {
    let _ = keychain::delete_api_key("noren-pro-token");
    let _ = keychain::delete_api_key("noren-pro-email");
    Ok(())
}

// --- Google OAuth ---

#[derive(Serialize)]
pub struct GoogleOAuthInitResult {
    pub auth_url: String,
    pub session_id: String,
}

#[tauri::command]
pub async fn google_oauth_init(
    state: State<'_, AppState>,
) -> Result<GoogleOAuthInitResult, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/google/init", server_url))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Google sign-in not available: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(GoogleOAuthInitResult {
        auth_url: data["auth_url"]
            .as_str()
            .ok_or("No auth_url in response")?
            .to_string(),
        session_id: data["session_id"]
            .as_str()
            .ok_or("No session_id in response")?
            .to_string(),
    })
}

#[derive(Serialize)]
pub struct GoogleOAuthPollResult {
    pub status: String,
    pub complete: bool,
}

#[tauri::command]
pub async fn google_oauth_poll(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<GoogleOAuthPollResult, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/auth/google/poll?session_id={}",
            server_url, session_id
        ))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Poll failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let status = data["status"]
        .as_str()
        .unwrap_or("pending")
        .to_string();

    if status == "complete" {
        let access_token = data["access_token"]
            .as_str()
            .ok_or("No access_token in poll response")?;
        let email = data["email"]
            .as_str()
            .ok_or("No email in poll response")?;

        keychain::store_api_key("noren-pro-token", access_token)?;
        keychain::store_api_key("noren-pro-email", email)?;
    }

    Ok(GoogleOAuthPollResult {
        status: status.clone(),
        complete: status == "complete",
    })
}

#[tauri::command]
pub async fn get_noren_pro_usage(
    state: State<'_, AppState>,
) -> Result<NorenProStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;
    let email = keychain::get_api_key("noren-pro-email");

    let proxy = noren_engine::NorenProxyClient::new(
        server_url.to_string(),
        auth_token,
        "general".to_string(),
    );

    let (used, limit, requests) = proxy.get_usage().await.map_err(|e| e.to_string())?;

    Ok(NorenProStatus {
        logged_in: true,
        email,
        inference_mode: "noren_pro".to_string(),
        tokens_used: Some(used),
        tokens_limit: Some(limit),
        requests_this_month: Some(requests),
    })
}

#[tauri::command]
pub fn set_inference_mode(
    state: State<'_, AppState>,
    mode: String,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.inference_mode = match mode.as_str() {
        "noren_pro" => noren_engine::InferenceMode::NorenPro,
        _ => noren_engine::InferenceMode::Byok,
    };
    save_config_file(&config);
    Ok(())
}

/// Persist config to ~/.noren/config.json
pub fn save_config_file(config: &noren_engine::Config) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let config_dir = std::path::PathBuf::from(home).join(".noren");
    let _ = std::fs::create_dir_all(&config_dir);

    let mut json = serde_json::json!({
        "provider": config.provider,
        "profileDir": config.profile_dir.to_string_lossy(),
        "inferenceMode": match config.inference_mode {
            noren_engine::InferenceMode::NorenPro => "noren_pro",
            noren_engine::InferenceMode::Byok => "byok",
        },
        "livingProfileEnabled": config.living_profile_enabled,
    });

    if let Some(ref url) = config.server_url {
        json["serverUrl"] = serde_json::Value::String(url.clone());
    }

    let _ = std::fs::write(
        config_dir.join("config.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
}
