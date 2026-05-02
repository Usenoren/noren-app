use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{hotkey, keychain, AppState};

#[derive(Serialize)]
pub struct SettingsInfo {
    pub provider: noren_engine::ProviderConfig,
    pub has_key: bool,
    pub inference_mode: String,
    pub noren_pro_logged_in: bool,
    pub hotkey: String,
    pub server_url: Option<String>,
    pub debug_mode: bool,
    pub theme: String,
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
        hotkey: config.hotkey.clone(),
        server_url: config.server_url.clone(),
        debug_mode: config.debug_mode,
        theme: config.theme.clone(),
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
pub fn set_provider(state: State<'_, AppState>, provider: SetProviderArgs) -> Result<(), String> {
    let provider_config =
        if let Some(preset) = noren_engine::ProviderConfig::preset_by_name(&provider.name) {
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
                base_url: provider.base_url.unwrap_or_default(),
                model: provider.model.unwrap_or_default(),
                requires_key: provider.requires_key.unwrap_or(true),
            }
        };

    let mut config = state.config.lock().unwrap();
    config.provider = provider_config;
    save_config_file(&config)?;

    Ok(())
}

#[tauri::command]
pub fn update_model(state: State<'_, AppState>, model: String) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.provider.model = model;
    save_config_file(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> Result<(), String> {
    const VALID: &[&str] = &[
        "kon", "charcoal", "classic", "sumi", "washi", "matcha", "kumo", "yoru",
    ];
    if !VALID.contains(&theme.as_str()) {
        return Err(format!("Unknown theme: {}", theme));
    }
    let mut config = state.config.lock().unwrap();
    config.theme = theme;
    save_config_file(&config)
}

#[tauri::command]
pub fn update_base_url(state: State<'_, AppState>, base_url: String) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.provider.base_url = base_url;
    save_config_file(&config)?;
    Ok(())
}

#[tauri::command]
pub fn save_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let keychain_id = config.provider.keychain_id();
    keychain::store_api_key(&keychain_id, &key)
}

#[tauri::command]
pub fn remove_api_key(state: State<'_, AppState>) -> Result<(), String> {
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
        let k = key.or_else(|| keychain::get_api_key(&provider.keychain_id()));
        if k.is_none() {
            return Err(format!("No API key for {}", provider.name));
        }
        k
    } else {
        None
    };

    let client = noren_engine::create_llm_client(&config, api_key).map_err(|e| e.to_string())?;

    let messages = vec![noren_engine::LlmMessage {
        role: noren_engine::Role::User,
        content: "Say 'ok'".to_string(),
    }];
    let options = noren_engine::LlmOptions {
        temperature: Some(0.0),
        max_tokens: Some(5),
        thinking: None,
        ..Default::default()
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
    pub email_verified: bool,
    pub inference_mode: String,
    pub tokens_used: Option<u64>,
    pub tokens_limit: Option<u64>,
    pub requests_this_month: Option<u64>,
    pub generations_used: Option<u64>,
    pub generations_limit: Option<u64>,
}

#[tauri::command]
pub fn get_noren_pro_status(state: State<'_, AppState>) -> NorenProStatus {
    let config = state.config.lock().unwrap();
    let has_token = keychain::get_api_key("noren-pro-token").is_some();
    let email = keychain::get_api_key("noren-pro-email");
    let email_verified = keychain::get_api_key("noren-pro-email-verified")
        .map(|v| v == "true")
        .unwrap_or(has_token);

    let mode = match config.inference_mode {
        noren_engine::InferenceMode::NorenPro => "noren_pro",
        noren_engine::InferenceMode::Byok => "byok",
    };

    NorenProStatus {
        logged_in: has_token,
        email,
        email_verified,
        inference_mode: mode.to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
        generations_used: None,
        generations_limit: None,
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
        .unwrap_or("https://api.usenoren.ai");

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
    let refresh = data["refresh_token"].as_str().unwrap_or("");
    let email_verified = data["email_verified"].as_bool().unwrap_or(false);

    // Store tokens and email in keychain
    keychain::store_api_key("noren-pro-token", token)?;
    if !refresh.is_empty() {
        keychain::store_api_key("noren-pro-refresh", refresh)?;
    }
    keychain::store_api_key("noren-pro-email", &email)?;
    keychain::store_api_key(
        "noren-pro-email-verified",
        if email_verified { "true" } else { "false" },
    )?;

    Ok(NorenProStatus {
        logged_in: true,
        email: Some(email),
        email_verified,
        inference_mode: "noren_pro".to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
        generations_used: None,
        generations_limit: None,
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
        .unwrap_or("https://api.usenoren.ai");

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
    let refresh = data["refresh_token"].as_str().unwrap_or("");
    let email_verified = data["email_verified"].as_bool().unwrap_or(false);

    // Store tokens and email in keychain
    keychain::store_api_key("noren-pro-token", token)?;
    if !refresh.is_empty() {
        keychain::store_api_key("noren-pro-refresh", refresh)?;
    }
    keychain::store_api_key("noren-pro-email", &email)?;
    keychain::store_api_key(
        "noren-pro-email-verified",
        if email_verified { "true" } else { "false" },
    )?;

    Ok(NorenProStatus {
        logged_in: true,
        email: Some(email),
        email_verified,
        inference_mode: "noren_pro".to_string(),
        tokens_used: None,
        tokens_limit: None,
        requests_this_month: None,
        generations_used: None,
        generations_limit: None,
    })
}

#[tauri::command]
pub fn noren_pro_logout() -> Result<(), String> {
    crate::auth_client::clear_auth_credentials();
    Ok(())
}

// --- Email OTP verification ---

#[tauri::command]
pub async fn verify_email(state: State<'_, AppState>, code: String) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let verify_url = format!("{}/v1/auth/verify-email", server_url);
    let payload = serde_json::json!({ "code": code });
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&verify_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&payload)
    })
    .await
    .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Verification failed: {}", body));
    }
    keychain::store_api_key("noren-pro-email-verified", "true")?;

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(data["message"]
        .as_str()
        .unwrap_or("Email verified")
        .to_string())
}

#[tauri::command]
pub async fn resend_otp(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let resend_url = format!("{}/v1/auth/resend-otp", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&resend_url)
            .header("Authorization", format!("Bearer {}", token))
    })
    .await
    .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to resend: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(data["message"].as_str().unwrap_or("Code sent").to_string())
}

#[tauri::command]
pub async fn resend_setup_email(
    state: State<'_, AppState>,
    email: String,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/resend-setup", server_url))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if resp.status().is_server_error() {
        return Err("Server error, please try again later".to_string());
    }

    // Return success for all other statuses (anti-enumeration)
    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(data["message"]
        .as_str()
        .unwrap_or("If that email is in our system, we've sent setup instructions.")
        .to_string())
}

// --- Password Reset ---

#[tauri::command]
pub async fn request_password_reset(
    state: State<'_, AppState>,
    email: String,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/request-password-reset", server_url))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if resp.status().is_server_error() {
        return Err("Server error, please try again later".to_string());
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(data["message"]
        .as_str()
        .unwrap_or("If that email exists, a reset code has been sent.")
        .to_string())
}

#[tauri::command]
pub async fn reset_password(
    state: State<'_, AppState>,
    email: String,
    code: String,
    new_password: String,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/reset-password", server_url))
        .json(&serde_json::json!({
            "email": email,
            "code": code,
            "new_password": new_password,
        }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(detail) = data["detail"].as_str() {
                return Err(detail.to_string());
            }
        }
        return Err("Password reset failed. Check the code and try again.".to_string());
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(data["message"]
        .as_str()
        .unwrap_or("Password reset successfully. Please log in with your new password.")
        .to_string())
}

#[tauri::command]
pub async fn change_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let change_url = format!("{}/v1/auth/change-password", server_url);
    let payload = serde_json::json!({
        "current_password": current_password,
        "new_password": new_password,
    });
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&change_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&payload)
    })
    .await
    .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Password change failed: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = data["access_token"]
        .as_str()
        .ok_or("No access token in response")?;
    let refresh = data["refresh_token"].as_str().unwrap_or("");
    keychain::store_api_key("noren-pro-token", token)?;
    if !refresh.is_empty() {
        keychain::store_api_key("noren-pro-refresh", refresh)?;
    }
    if let Some(email_verified) = data["email_verified"].as_bool() {
        keychain::store_api_key(
            "noren-pro-email-verified",
            if email_verified { "true" } else { "false" },
        )?;
    }

    Ok("Password changed".to_string())
}

// --- Account Deletion ---

#[tauri::command]
pub async fn request_delete_account(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let request_url = format!("{}/v1/auth/request-account-deletion", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&request_url)
            .header("Authorization", format!("Bearer {}", token))
    })
    .await
    .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to request deletion: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(data["message"]
        .as_str()
        .unwrap_or("Verification code sent")
        .to_string())
}

#[tauri::command]
pub async fn confirm_delete_account(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let delete_url = format!("{}/v1/auth/delete-account", server_url);
    let payload = serde_json::json!({ "code": code });
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&delete_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&payload)
    })
    .await
    .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Deletion failed: {}", body));
    }

    // Clean up local credentials
    crate::auth_client::clear_auth_credentials();

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(data["message"]
        .as_str()
        .unwrap_or("Account deleted")
        .to_string())
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
        .unwrap_or("https://api.usenoren.ai");

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
        .unwrap_or("https://api.usenoren.ai");

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

    let status = data["status"].as_str().unwrap_or("pending").to_string();

    if status == "complete" {
        let access_token = data["access_token"]
            .as_str()
            .ok_or("No access_token in poll response")?;
        let refresh_token = data["refresh_token"].as_str().unwrap_or("");
        let email = data["email"].as_str().ok_or("No email in poll response")?;

        keychain::store_api_key("noren-pro-token", access_token)?;
        if !refresh_token.is_empty() {
            keychain::store_api_key("noren-pro-refresh", refresh_token)?;
        }
        keychain::store_api_key("noren-pro-email", email)?;
        keychain::store_api_key("noren-pro-email-verified", "true")?;
    }

    Ok(GoogleOAuthPollResult {
        status: status.clone(),
        complete: status == "complete",
    })
}

#[tauri::command]
pub async fn get_noren_pro_usage(state: State<'_, AppState>) -> Result<NorenProStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token").ok_or("Not logged in")?;
    let email = keychain::get_api_key("noren-pro-email");
    let email_verified = keychain::get_api_key("noren-pro-email-verified")
        .map(|v| v == "true")
        .unwrap_or(true);

    let refresh_token = keychain::get_api_key("noren-pro-refresh");
    let mut proxy = noren_engine::NorenProxyClient::new(
        server_url.to_string(),
        auth_token,
        "general".to_string(),
    );
    if let Some(rt) = refresh_token {
        proxy = proxy.with_token_refresh(rt, |new_access, new_refresh| {
            let _ = keychain::store_api_key("noren-pro-token", &new_access);
            let _ = keychain::store_api_key("noren-pro-refresh", &new_refresh);
        });
    }

    let (used, limit, requests, gen_used, gen_limit) = proxy
        .get_usage()
        .await
        .map_err(|e| crate::auth_client::normalize_auth_error(e.to_string()))?;

    Ok(NorenProStatus {
        logged_in: true,
        email,
        email_verified,
        inference_mode: "noren_pro".to_string(),
        tokens_used: Some(used),
        tokens_limit: Some(limit),
        requests_this_month: Some(requests),
        generations_used: Some(gen_used),
        generations_limit: Some(gen_limit),
    })
}

#[tauri::command]
pub fn set_inference_mode(state: State<'_, AppState>, mode: String) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.inference_mode = match mode.as_str() {
        "noren_pro" => noren_engine::InferenceMode::NorenPro,
        _ => noren_engine::InferenceMode::Byok,
    };
    save_config_file(&config)?;
    Ok(())
}

#[tauri::command]
pub fn update_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    hotkey_str: String,
) -> Result<(), String> {
    // Validate first
    hotkey::parse_shortcut(&hotkey_str)?;
    // Swap the live shortcut
    hotkey::re_register(&app, &hotkey_str)?;
    // Persist
    let mut config = state.config.lock().unwrap();
    config.hotkey = hotkey_str;
    save_config_file(&config)?;
    Ok(())
}

// --- Ollama model discovery ---

#[tauri::command]
pub async fn list_ollama_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let config = state.config.lock().unwrap().clone();
    let base_url = config
        .provider
        .base_url
        .trim_end_matches("/v1")
        .trim_end_matches("/v1/")
        .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!("{}/api/tags", base_url))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama: {}", e))?;

    if !resp.status().is_success() {
        return Err("Ollama returned an error".to_string());
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let models = data["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

// --- Claude model discovery ---

#[derive(Serialize)]
pub struct ClaudeModelInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn list_claude_models(
    state: State<'_, AppState>,
) -> Result<Vec<ClaudeModelInfo>, String> {
    let config = state.config.lock().unwrap().clone();
    let provider = &config.provider;

    // Derive API root from base_url (strip /v1/messages)
    let base = provider
        .base_url
        .trim_end_matches("/v1/messages")
        .trim_end_matches("/v1/messages/")
        .to_string();

    let api_key = if provider.requires_key {
        keychain::get_api_key(&provider.keychain_id())
    } else {
        None
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .get(format!("{}/v1/models", base))
        .header("anthropic-version", "2023-06-01");

    if let Some(key) = api_key {
        if provider.name == "claude-token" {
            req = req
                .header("Authorization", format!("Bearer {}", key))
                .header("anthropic-beta", "oauth-2025-04-20");
        } else {
            req = req.header("x-api-key", key);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Cannot reach API: {}", e))?;
    if !resp.status().is_success() {
        return Err("Failed to fetch models".to_string());
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let models = data["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    if !id.starts_with("claude-") {
                        return None;
                    }
                    let name = m["display_name"].as_str().unwrap_or(id);
                    Some(ClaudeModelInfo {
                        id: id.to_string(),
                        name: name.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

// --- Gemini model discovery ---

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn list_gemini_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let config = state.config.lock().unwrap().clone();
    let provider = &config.provider;

    let api_key = if provider.requires_key {
        keychain::get_api_key(&provider.keychain_id())
    } else {
        None
    };
    let api_key = api_key.ok_or("No API key set")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}",
            api_key
        ))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Gemini API: {}", e))?;

    if !resp.status().is_success() {
        return Err("Failed to fetch Gemini models".to_string());
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let models = data["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m["name"].as_str()?;
                    let methods = m["supportedGenerationMethods"].as_array()?;
                    let supports_generate = methods
                        .iter()
                        .any(|v| v.as_str() == Some("generateContent"));
                    if !supports_generate {
                        return None;
                    }
                    // Filter out non-Gemini models (e.g. Gemma) that don't support system instructions
                    let id_str = name.strip_prefix("models/").unwrap_or(name);
                    if !id_str.starts_with("gemini-") {
                        return None;
                    }
                    let display = m["displayName"].as_str().unwrap_or(name);
                    let id = name.strip_prefix("models/").unwrap_or(name);
                    Some(ModelInfo {
                        id: id.to_string(),
                        name: display.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

// --- OpenAI model discovery ---

#[tauri::command]
pub async fn list_openai_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let config = state.config.lock().unwrap().clone();
    let provider = &config.provider;

    let api_key = if provider.requires_key {
        keychain::get_api_key(&provider.keychain_id())
    } else {
        None
    };
    let api_key = api_key.ok_or("No API key set")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Cannot reach OpenAI API: {}", e))?;

    if !resp.status().is_success() {
        return Err("Failed to fetch OpenAI models".to_string());
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut models: Vec<ModelInfo> = data["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    if !(id.starts_with("gpt-")
                        || id.starts_with("chatgpt-")
                        || (id.starts_with('o')
                            && id.chars().nth(1).is_some_and(|c| c.is_ascii_digit())))
                    {
                        return None;
                    }
                    if id.contains("instruct") || id.contains("audio") || id.contains("realtime") {
                        return None;
                    }
                    Some(ModelInfo {
                        id: id.to_string(),
                        name: id.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

// --- Custom model discovery ---

#[tauri::command]
pub async fn list_custom_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let config = state.config.lock().unwrap().clone();
    let provider = &config.provider;
    let base_url = provider.base_url.trim_end_matches('/');

    let api_key = if provider.requires_key {
        keychain::get_api_key(&provider.keychain_id())
    } else {
        None
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(format!("{}/models", base_url));
    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Cannot reach API: {}", e))?;
    if !resp.status().is_success() {
        return Err("Failed to fetch models".to_string());
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut models: Vec<ModelInfo> = data["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    let name = m["name"].as_str().unwrap_or(id);
                    Some(ModelInfo {
                        id: id.to_string(),
                        name: name.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

// --- Thinking settings ---

#[derive(Serialize)]
pub struct ThinkingSettings {
    pub enabled: bool,
    pub budget: u32,
}

#[tauri::command]
pub fn get_thinking_settings(state: State<'_, AppState>) -> ThinkingSettings {
    let config = state.config.lock().unwrap();
    ThinkingSettings {
        enabled: config.extended_thinking,
        budget: config.thinking_budget,
    }
}

#[tauri::command]
pub fn set_thinking_settings(
    state: State<'_, AppState>,
    enabled: bool,
    budget: u32,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.extended_thinking = enabled;
    config.thinking_budget = budget;
    save_config_file(&config)?;
    Ok(())
}

#[tauri::command]
pub fn factory_reset(state: State<'_, AppState>) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let noren_dir = std::path::PathBuf::from(home).join(".noren");

    // Delete ~/.noren/ entirely
    if noren_dir.exists() {
        std::fs::remove_dir_all(&noren_dir)
            .map_err(|e| format!("Failed to delete ~/.noren: {}", e))?;
    }

    // Clear all Noren keychain entries
    let keychain_accounts = [
        "noren-pro-token",
        "noren-pro-refresh",
        "noren-pro-email",
        "anthropic",
        "openai",
        "gemini",
        "claude-token",
        "custom",
    ];
    for account in keychain_accounts {
        let _ = keychain::delete_api_key(account);
    }
    // Keep the prompt-cache key so regenerated cache entries can continue to
    // use the stable keychain secret.

    // Reset in-memory config to defaults
    let mut config = state.config.lock().unwrap();
    *config = noren_engine::Config::default();

    Ok(())
}

/// Persist config to ~/.noren/config.json
pub fn save_config_file(config: &noren_engine::Config) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let config_dir = std::path::PathBuf::from(home).join(".noren");
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let mut json = serde_json::json!({
        "provider": config.provider,
        "profileDir": config.profile_dir.to_string_lossy(),
        "inferenceMode": match config.inference_mode {
            noren_engine::InferenceMode::NorenPro => "noren_pro",
            noren_engine::InferenceMode::Byok => "byok",
        },
        "livingProfileEnabled": config.living_profile_enabled,
        "hotkey": config.hotkey,
        "extendedThinking": config.extended_thinking,
        "thinkingBudget": config.thinking_budget,
        "debugMode": config.debug_mode,
        "theme": config.theme,
    });

    if let Some(ref url) = config.server_url {
        json["serverUrl"] = serde_json::Value::String(url.clone());
    }

    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(config_dir.join("config.json"), pretty)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}
