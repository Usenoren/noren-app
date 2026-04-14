use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

// Re-use save_config_file from settings
use super::settings::save_config_file;

#[derive(Serialize)]
pub struct LivingProfileStatus {
    pub enabled: bool,
    pub edit_count: u64,
    pub last_upload: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SectionDiff {
    pub section: String,
    pub before: String,
    pub after: String,
}

#[derive(Serialize, Deserialize)]
pub struct RefreshHistoryEntry {
    pub id: String,
    pub diffs: Vec<SectionDiff>,
    pub observations: Vec<String>,
    pub sections_updated: Vec<String>,
    pub edits_analyzed: u32,
    pub samples_analyzed: u32,
    pub generations_analyzed: u32,
    pub rolled_back: bool,
    pub created_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct RefreshResponse {
    pub refreshed: bool,
    pub sections_updated: Vec<String>,
    pub message: String,
    pub observations: Vec<String>,
    pub history_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub has_profile: bool,
    pub formats: Vec<String>,
    pub created_at: Option<String>,
    pub source: Option<String>,
    pub last_extracted_at: Option<String>,
    pub extraction_count: u32,
    pub next_refresh_available: Option<String>,
    pub can_rollback: bool,
    pub voice_overview: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExternalSample {
    pub text: String,
    pub format: String,
    pub added_at: String,
}

#[tauri::command]
pub fn get_living_profile_status(state: State<'_, AppState>) -> LivingProfileStatus {
    let config = state.config.lock().unwrap();
    let base_dir = config.profile_dir.parent().unwrap_or(&config.profile_dir);
    let logger = noren_engine::tracking::EditLogger::new(base_dir);
    let entries = logger.read_all();

    LivingProfileStatus {
        enabled: config.living_profile_enabled,
        edit_count: entries.len() as u64,
        last_upload: None, // TODO: track last upload time
    }
}

#[tauri::command]
pub fn set_living_profile_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.living_profile_enabled = enabled;
    save_config_file(&config)?;
    Ok(())
}

#[tauri::command]
pub fn log_edit(
    state: State<'_, AppState>,
    ctx: String,
    orig: String,
    edit: String,
    app: String,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();

    // Only log if living profile is explicitly enabled
    if !config.living_profile_enabled {
        return Ok(());
    }

    let base_dir = config.profile_dir.parent().unwrap_or(&config.profile_dir);
    let logger = noren_engine::tracking::EditLogger::new(base_dir);

    let entry = noren_engine::tracking::EditEntry {
        ts: chrono_now(),
        ctx,
        orig,
        edit,
        app,
    };

    logger.log(&entry).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_edit_log(
    state: State<'_, AppState>,
    external_samples: Option<Vec<ExternalSample>>,
) -> Result<u64, String> {
    let config = state.config.lock().unwrap().clone();

    if !config.living_profile_enabled {
        return Err("Living profile not enabled".to_string());
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    // Read local edit entries
    let base_dir = config.profile_dir.parent().unwrap_or(&config.profile_dir);
    let logger = noren_engine::tracking::EditLogger::new(base_dir);
    let entries = logger.read_all();

    let has_entries = !entries.is_empty();
    let has_samples = external_samples.as_ref().map_or(false, |s| !s.is_empty());

    if !has_entries && !has_samples {
        return Ok(0);
    }

    // Build request body
    let mut body = serde_json::Map::new();

    if has_entries {
        let entries_json: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "ts": e.ts,
                    "ctx": e.ctx,
                    "orig": e.orig,
                    "edit": e.edit,
                    "app": e.app,
                })
            })
            .collect();
        body.insert("entries".to_string(), serde_json::Value::Array(entries_json));
    }

    if has_samples {
        let samples_json: Vec<serde_json::Value> = external_samples
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| {
                serde_json::json!({
                    "text": s.text,
                    "format": s.format,
                    "added_at": s.added_at,
                })
            })
            .collect();
        body.insert("external_samples".to_string(), serde_json::Value::Array(samples_json));
    }

    let upload_url = format!("{}/v1/profile/upload-edits", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&upload_url)
            .bearer_auth(token)
            .json(&body)
    })
    .await
    .map_err(|e| format!("Upload failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Upload failed: {}", body));
    }

    Ok(entries.len() as u64)
}

#[tauri::command]
pub async fn refresh_living_profile(
    state: State<'_, AppState>,
) -> Result<RefreshResponse, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let refresh_url = format!("{}/v1/profile/refresh", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&refresh_url)
            .bearer_auth(token)
    })
    .await
    .map_err(|e| format!("Refresh failed: {}", e))?;

    let resp_status = resp.status();

    // Handle 429 rate limit
    if resp_status.as_u16() == 429 {
        let data: serde_json::Value = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e: reqwest::Error| e.to_string())?;
        let detail = data["detail"].as_str().unwrap_or("Rate limited");
        let retry_after = data["retry_after"].as_str().unwrap_or("");
        return Err(format!("Rate limited: {}. Retry after: {}", detail, retry_after));
    }

    if !resp_status.is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Refresh failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(RefreshResponse {
        refreshed: data["refreshed"].as_bool().unwrap_or(false),
        sections_updated: data["sections_updated"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        message: data["message"].as_str().unwrap_or("").to_string(),
        observations: data["observations"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        history_id: data["history_id"].as_str().map(|s| s.to_string()),
    })
}

#[tauri::command]
pub async fn get_profile_metadata(
    state: State<'_, AppState>,
) -> Result<ProfileMetadata, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let metadata_url = format!("{}/v1/profile/voice/metadata", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .get(&metadata_url)
            .bearer_auth(token)
    })
    .await
    .map_err(|e| format!("Failed to get metadata: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get metadata: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(ProfileMetadata {
        has_profile: data["has_profile"].as_bool().unwrap_or(false),
        formats: data["formats"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        created_at: data["created_at"].as_str().map(|s| s.to_string()),
        source: data["source"].as_str().map(|s| s.to_string()),
        last_extracted_at: data["last_extracted_at"].as_str().map(|s| s.to_string()),
        extraction_count: data["extraction_count"].as_u64().unwrap_or(1) as u32,
        next_refresh_available: data["next_refresh_available"].as_str().map(|s| s.to_string()),
        can_rollback: data["can_rollback"].as_bool().unwrap_or(false),
        voice_overview: data.get("voice_overview").cloned(),
    })
}

#[tauri::command]
pub async fn rollback_profile(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let rollback_url = format!("{}/v1/profile/voice/rollback", server_url);
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&rollback_url)
            .bearer_auth(token)
    })
    .await
    .map_err(|e| format!("Rollback failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Rollback failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(data["message"].as_str().unwrap_or("Profile restored").to_string())
}

#[tauri::command]
pub async fn get_refresh_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<RefreshHistoryEntry>, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let limit = limit.unwrap_or(20);
    let offset = offset.unwrap_or(0);

    let history_url = format!(
        "{}/v1/profile/refresh-history?limit={}&offset={}",
        server_url, limit, offset
    );
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .get(&history_url)
            .bearer_auth(token)
    })
    .await
    .map_err(|e| format!("Failed to get history: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get history: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let entries: Vec<RefreshHistoryEntry> = data["entries"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| {
            Some(RefreshHistoryEntry {
                id: e["id"].as_str()?.to_string(),
                diffs: e["diffs"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|d| {
                        Some(SectionDiff {
                            section: d["section"].as_str()?.to_string(),
                            before: d["before"].as_str()?.to_string(),
                            after: d["after"].as_str()?.to_string(),
                        })
                    })
                    .collect(),
                observations: e["observations"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                sections_updated: e["sections_updated"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                edits_analyzed: e["edits_analyzed"].as_u64().unwrap_or(0) as u32,
                samples_analyzed: e["samples_analyzed"].as_u64().unwrap_or(0) as u32,
                generations_analyzed: e["generations_analyzed"].as_u64().unwrap_or(0) as u32,
                rolled_back: e["rolled_back"].as_bool().unwrap_or(false),
                created_at: e["created_at"].as_str()?.to_string(),
            })
        })
        .collect();

    Ok(entries)
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert epoch seconds to ISO-8601 date string
    let days = secs / 86400;
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let dy = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < dy { break; }
        remaining -= dy;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u32;
    for &d in &month_days {
        if remaining < d as i64 { break; }
        remaining -= d as i64;
        m += 1;
    }
    let day_secs = secs % 86400;
    let h = day_secs / 3600;
    let min = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, remaining + 1, h, min, s)
}
