use serde::{Deserialize, Serialize};

use crate::AppState;
use super::billing::server_url_from_config;

#[derive(Serialize, Deserialize)]
pub struct ScrapeMeta {
    pub source_type: String,
    pub total_found: usize,
    pub total_kept: usize,
    pub total_discarded: usize,
    #[serde(default)]
    pub ai_filtered: usize,
    #[serde(default)]
    pub years_spanned: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct ScrapeFormatGroup {
    pub format: String,
    pub samples: String,
}

#[derive(Serialize, Deserialize)]
pub struct ScrapeResponse {
    pub format_group: ScrapeFormatGroup,
    pub meta: ScrapeMeta,
}

#[tauri::command]
pub async fn scrape_twitter(
    state: tauri::State<'_, AppState>,
    handle: String,
    count: Option<u32>,
) -> Result<ScrapeResponse, String> {
    let server_url = server_url_from_config(&state);
    let body = serde_json::json!({
        "handle": handle,
        "count": count.unwrap_or(100),
    });

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/scrape/twitter", server_url))
            .bearer_auth(token)
            .json(&body)
    })
    .await?;

    let status = resp.status();

    if status.as_u16() == 401 {
        return Err("Please sign in to use this feature.".to_string());
    }

    if status.as_u16() == 429 {
        return Err("Rate limit reached. Twitter scraping is limited to 3 times per hour.".to_string());
    }

    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| v["detail"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("Server error ({})", status.as_u16()));
        return Err(detail);
    }

    resp.json::<ScrapeResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tauri::command]
pub async fn scrape_blog(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<ScrapeResponse, String> {
    let server_url = server_url_from_config(&state);
    let body = serde_json::json!({ "url": url });

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/scrape/blog", server_url))
            .bearer_auth(token)
            .json(&body)
    })
    .await?;

    let status = resp.status();

    if status.as_u16() == 401 {
        return Err("Please sign in to use this feature.".to_string());
    }

    if status.as_u16() == 429 {
        return Err("Rate limit reached. Blog scraping is limited to 10 times per hour.".to_string());
    }

    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| v["detail"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("Server error ({})", status.as_u16()));
        return Err(detail);
    }

    resp.json::<ScrapeResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tauri::command]
pub async fn scrape_reddit(
    state: tauri::State<'_, AppState>,
    username: String,
) -> Result<ScrapeResponse, String> {
    let server_url = server_url_from_config(&state);
    let body = serde_json::json!({ "username": username });

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/scrape/reddit", server_url))
            .bearer_auth(token)
            .json(&body)
    })
    .await?;

    let status = resp.status();

    if status.as_u16() == 401 {
        return Err("Please sign in to use this feature.".to_string());
    }

    if status.as_u16() == 429 {
        return Err("Rate limit reached. Reddit scraping is limited to 5 times per hour.".to_string());
    }

    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| v["detail"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("Server error ({})", status.as_u16()));
        return Err(detail);
    }

    resp.json::<ScrapeResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}
