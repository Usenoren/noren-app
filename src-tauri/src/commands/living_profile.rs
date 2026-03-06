use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{keychain, AppState};

// Re-use save_config_file from settings
use super::settings::save_config_file;

#[derive(Serialize)]
pub struct LivingProfileStatus {
    pub enabled: bool,
    pub edit_count: u64,
    pub last_upload: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ProfilePatch {
    pub patch_id: String,
    pub section: String,
    pub change_type: String,
    pub description: String,
    pub original_text: Option<String>,
    pub new_text: Option<String>,
    pub confidence: f64,
    pub status: String,
}

#[derive(Serialize)]
pub struct RefreshResult {
    pub patches: Vec<ProfilePatch>,
    pub signals_found: u64,
    pub entries_analyzed: u64,
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
) -> Result<u64, String> {
    let config = state.config.lock().unwrap().clone();

    if !config.living_profile_enabled {
        return Err("Living profile not enabled".to_string());
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    // Read local edit entries
    let base_dir = config.profile_dir.parent().unwrap_or(&config.profile_dir);
    let logger = noren_engine::tracking::EditLogger::new(base_dir);
    let entries = logger.read_all();

    if entries.is_empty() {
        return Ok(0);
    }

    // Convert to JSON-friendly format
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

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/profile/upload-edits", server_url))
        .bearer_auth(&auth_token)
        .json(&serde_json::json!({ "entries": entries_json }))
        .send()
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
) -> Result<RefreshResult, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/profile/refresh", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Refresh failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Refresh failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let patches: Vec<ProfilePatch> = data["patches"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|p| {
            Some(ProfilePatch {
                patch_id: p["patch_id"].as_str()?.to_string(),
                section: p["section"].as_str()?.to_string(),
                change_type: p["change_type"].as_str()?.to_string(),
                description: p["description"].as_str()?.to_string(),
                original_text: p["original_text"].as_str().map(|s| s.to_string()),
                new_text: p["new_text"].as_str().map(|s| s.to_string()),
                confidence: p["confidence"].as_f64().unwrap_or(0.0),
                status: p["status"].as_str().unwrap_or("pending").to_string(),
            })
        })
        .collect();

    Ok(RefreshResult {
        patches,
        signals_found: data["signals_found"].as_u64().unwrap_or(0),
        entries_analyzed: data["entries_analyzed"].as_u64().unwrap_or(0),
    })
}

#[tauri::command]
pub async fn get_profile_patches(
    state: State<'_, AppState>,
) -> Result<Vec<ProfilePatch>, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .get(format!("{}/v1/profile/patches", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Failed to get patches: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get patches: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let patches: Vec<ProfilePatch> = data["patches"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|p| {
            Some(ProfilePatch {
                patch_id: p["patch_id"].as_str()?.to_string(),
                section: p["section"].as_str()?.to_string(),
                change_type: p["change_type"].as_str()?.to_string(),
                description: p["description"].as_str()?.to_string(),
                original_text: p["original_text"].as_str().map(|s| s.to_string()),
                new_text: p["new_text"].as_str().map(|s| s.to_string()),
                confidence: p["confidence"].as_f64().unwrap_or(0.0),
                status: p["status"].as_str().unwrap_or("pending").to_string(),
            })
        })
        .collect();

    Ok(patches)
}

#[tauri::command]
pub async fn approve_profile_patch(
    state: State<'_, AppState>,
    patch_id: String,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/profile/patches/{}/approve", server_url, patch_id))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed: {}", body));
    }

    Ok(())
}

#[tauri::command]
pub async fn reject_profile_patch(
    state: State<'_, AppState>,
    patch_id: String,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .post(format!("{}/v1/profile/patches/{}/reject", server_url, patch_id))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed: {}", body));
    }

    Ok(())
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
