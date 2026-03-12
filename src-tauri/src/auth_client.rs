//! Shared authenticated HTTP client with automatic 401 token refresh.
//!
//! All Tauri commands that call the Noren server should use `authed_request()`
//! instead of building raw reqwest requests. This ensures tokens are refreshed
//! transparently when the 30-minute access token expires.

use crate::keychain;

/// Get the current auth token or return an error.
pub fn require_auth() -> Result<String, String> {
    keychain::get_api_key("noren-pro-token").ok_or_else(|| "Not logged in".to_string())
}

/// Send an authenticated request with automatic 401 retry via refresh token.
///
/// `build_request` is called with (client, auth_token) and should return a
/// ready-to-send RequestBuilder. If the first attempt returns 401, the refresh
/// token is used to obtain new tokens, and the request is retried once.
pub async fn authed_request<F>(
    server_url: &str,
    build_request: F,
) -> Result<reqwest::Response, String>
where
    F: Fn(&reqwest::Client, &str) -> reqwest::RequestBuilder,
{
    let auth_token = require_auth()?;
    let client = reqwest::Client::new();

    let resp = build_request(&client, &auth_token)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if resp.status().as_u16() != 401 {
        return Ok(resp);
    }

    // Attempt token refresh
    let refresh_token = match keychain::get_api_key("noren-pro-refresh") {
        Some(rt) => rt,
        None => return Ok(resp), // No refresh token, return the 401 as-is
    };

    let refresh_resp = client
        .post(format!("{}/v1/auth/refresh", server_url))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    if !refresh_resp.status().is_success() {
        // Refresh failed, return original 401
        return Err("Session expired. Please sign in again.".to_string());
    }

    let data: serde_json::Value = refresh_resp
        .json()
        .await
        .map_err(|e| format!("Invalid refresh response: {}", e))?;

    let new_access = data["access_token"]
        .as_str()
        .ok_or("No access_token in refresh response")?;
    let new_refresh = data["refresh_token"]
        .as_str()
        .unwrap_or("");

    // Persist new tokens
    keychain::store_api_key("noren-pro-token", new_access)?;
    if !new_refresh.is_empty() {
        keychain::store_api_key("noren-pro-refresh", new_refresh)?;
    }

    // Retry the original request with the new token
    build_request(&client, new_access)
        .send()
        .await
        .map_err(|e| format!("Retry failed: {}", e))
}
