use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
pub struct ProfileOverview {
    pub exists: bool,
    pub path: String,
    pub formats: Vec<String>,
    /// True when profile is stored on Noren servers (Pro path)
    pub is_server: bool,
}

#[derive(Deserialize)]
struct ServerProfileMetadata {
    has_profile: bool,
    formats: Vec<String>,
    #[allow(dead_code)]
    created_at: Option<String>,
    #[allow(dead_code)]
    source: Option<String>,
}

#[tauri::command]
pub async fn get_profile_overview(state: State<'_, AppState>) -> Result<ProfileOverview, String> {
    let config = state.config.lock().unwrap().clone();

    // Pro path: check server for profile
    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        if let Some(auth_token) = crate::keychain::get_api_key("noren-pro-token") {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.noren.ink");

            match fetch_server_profile_metadata(server_url, &auth_token).await {
                Ok(meta) if meta.has_profile => {
                    return Ok(ProfileOverview {
                        exists: true,
                        path: String::new(),
                        formats: meta.formats,
                        is_server: true,
                    });
                }
                Ok(meta) if !meta.has_profile && !config.debug_mode => {
                    // Production: server is authoritative — no profile means no profile
                    return Ok(ProfileOverview {
                        exists: false,
                        path: String::new(),
                        formats: vec![],
                        is_server: true,
                    });
                }
                _ => {
                    // Debug mode or server unreachable — fall through to local check
                }
            }
        }
    }

    // BYOK / fallback: check local profile
    let dir = &config.profile_dir;
    let exists = dir.join("core-identity.md").exists();
    let formats = if exists {
        noren_engine::list_formats(dir)
    } else {
        vec![]
    };
    Ok(ProfileOverview {
        exists,
        path: dir.to_string_lossy().to_string(),
        formats,
        is_server: false,
    })
}

#[tauri::command]
pub fn read_profile_content(
    state: State<'_, AppState>,
) -> Result<noren_engine::ProfileContent, String> {
    let config = state.config.lock().unwrap();

    // Server profiles can't be read locally — use Export to download first
    if config.inference_mode == noren_engine::InferenceMode::NorenPro
        && !config.profile_dir.join("core-identity.md").exists()
    {
        return Err("Profile is stored on Noren servers. Use Export to download.".to_string());
    }

    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    let qc_path = config.profile_dir.join("quality-check-results.md");
    let quality_check = std::fs::read_to_string(qc_path).ok();

    Ok(noren_engine::ProfileContent {
        core_identity,
        contexts,
        quality_check,
    })
}

#[tauri::command]
pub fn save_profile_edit(
    state: State<'_, AppState>,
    core_identity: String,
    context_format: Option<String>,
    context_content: Option<String>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let dir = &config.profile_dir;

    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create profile directory: {}", e))?;

    std::fs::write(dir.join("core-identity.md"), &core_identity)
        .map_err(|e| format!("Failed to save core identity: {}", e))?;

    if let (Some(fmt), Some(content)) = (context_format, context_content) {
        let contexts_dir = dir.join("contexts");
        let _ = std::fs::create_dir_all(&contexts_dir);
        std::fs::write(contexts_dir.join(format!("{}.md", fmt)), &content)
            .map_err(|e| format!("Failed to save context: {}", e))?;
    }

    Ok(())
}

/// Migrate a local profile to Noren servers.
/// Uploads the local profile, then deletes the local copy.
#[tauri::command]
pub async fn migrate_profile_to_server(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();

    let auth_token = crate::keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in to Noren Pro.")?;

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    // Load local profile
    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    let qc_path = config.profile_dir.join("quality-check-results.md");
    let quality_report = std::fs::read_to_string(qc_path).ok();

    // Upload to server
    let client = reqwest::Client::new();
    let resp = client
        .put(format!("{}/v1/profile/voice", server_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "core_identity": core_identity,
            "contexts": contexts,
            "quality_report": quality_report,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to upload profile: {}", e))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Server rejected profile upload: {}", text));
    }

    // Delete local profile
    let _ = std::fs::remove_dir_all(&config.profile_dir);

    Ok("Profile migrated to Noren servers".to_string())
}

/// Export server-side profile to local disk.
#[tauri::command]
pub async fn export_profile(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();

    let auth_token = crate::keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in to Noren Pro.")?;

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/profile/voice/export", server_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .map_err(|e| format!("Failed to export profile: {}", e))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Export failed: {}", text));
    }

    #[derive(Deserialize)]
    struct ExportResponse {
        core_identity: String,
        contexts: std::collections::HashMap<String, String>,
        quality_report: Option<String>,
    }

    let export: ExportResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse export: {}", e))?;

    // Save to local disk
    noren_engine::save_profile(
        &config.profile_dir,
        &export.core_identity,
        &export.contexts,
        &export.quality_report.unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    Ok(config.profile_dir.to_string_lossy().to_string())
}

/// Fetch profile metadata from the Noren server.
async fn fetch_server_profile_metadata(
    server_url: &str,
    auth_token: &str,
) -> Result<ServerProfileMetadata, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/profile/voice/metadata", server_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err("Failed to fetch profile metadata".to_string());
    }

    resp.json::<ServerProfileMetadata>()
        .await
        .map_err(|e| e.to_string())
}
