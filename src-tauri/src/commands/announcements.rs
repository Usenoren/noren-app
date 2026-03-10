use tauri::State;

use crate::AppState;
use super::settings::save_config_file;
use super::billing::server_url_from_config;

#[tauri::command]
pub async fn fetch_announcements(
    state: State<'_, AppState>,
    since: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let server_url = server_url_from_config(&state);
    let url = match since {
        Some(ref s) if !s.is_empty() => format!("{}/v1/announcements?since={}", server_url, s),
        _ => format!("{}/v1/announcements", server_url),
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch announcements: {}", e))?;

    if !resp.status().is_success() {
        return Ok(vec![]);
    }

    resp.json::<Vec<serde_json::Value>>()
        .await
        .map_err(|e| format!("Failed to parse announcements: {}", e))
}

#[tauri::command]
pub fn get_announcement_seen(state: State<'_, AppState>) -> Option<String> {
    let config = state.config.lock().unwrap();
    config.last_seen_announcement_ts.clone()
}

#[tauri::command]
pub fn save_announcement_seen(
    state: State<'_, AppState>,
    ts: String,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.last_seen_announcement_ts = Some(ts);
    save_config_file(&config)
}
