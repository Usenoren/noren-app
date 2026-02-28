use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{keychain, AppState};

#[derive(Serialize, Deserialize)]
pub struct SubscriptionStatus {
    pub tier: String,
    pub active: bool,
    pub can_extract: bool,
    pub can_generate_bundled: bool,
    pub can_living_profile: bool,
    pub can_sync: bool,
    pub tokens_limit: u64,
    pub current_period_end: Option<String>,
    pub cancel_at_period_end: bool,
}

#[derive(Serialize)]
pub struct CheckoutResult {
    pub checkout_url: String,
    pub session_id: String,
}

#[tauri::command]
pub async fn get_subscription_status(
    state: State<'_, AppState>,
) -> Result<SubscriptionStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .get(format!("{}/v1/billing/status", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get subscription: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let ents = &data["entitlements"];

    Ok(SubscriptionStatus {
        tier: data["tier"].as_str().unwrap_or("free").to_string(),
        active: data["active"].as_bool().unwrap_or(false),
        can_extract: ents["can_extract"].as_bool().unwrap_or(false),
        can_generate_bundled: ents["can_generate_bundled"].as_bool().unwrap_or(false),
        can_living_profile: ents["can_living_profile"].as_bool().unwrap_or(false),
        can_sync: ents["can_sync"].as_bool().unwrap_or(false),
        tokens_limit: ents["tokens_limit"].as_u64().unwrap_or(0),
        current_period_end: data["current_period_end"].as_str().map(|s| s.to_string()),
        cancel_at_period_end: data["cancel_at_period_end"].as_bool().unwrap_or(false),
    })
}

#[tauri::command]
pub async fn create_checkout(
    state: State<'_, AppState>,
    tier: String,
) -> Result<CheckoutResult, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in — sign in first")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/billing/checkout", server_url))
        .bearer_auth(&auth_token)
        .json(&serde_json::json!({ "tier": tier }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Checkout failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(CheckoutResult {
        checkout_url: data["checkout_url"]
            .as_str()
            .ok_or("No checkout URL in response")?
            .to_string(),
        session_id: data["session_id"]
            .as_str()
            .ok_or("No session ID in response")?
            .to_string(),
    })
}

#[tauri::command]
pub async fn open_billing_portal(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/billing/portal", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Portal failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    data["portal_url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No portal URL in response".to_string())
}
