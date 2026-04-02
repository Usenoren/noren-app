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
    /// Curated voice metadata for frontend visualization
    pub voice_overview: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ServerProfileMetadata {
    has_profile: bool,
    formats: Vec<String>,
    #[allow(dead_code)]
    created_at: Option<String>,
    #[allow(dead_code)]
    source: Option<String>,
    voice_overview: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn get_profile_overview(state: State<'_, AppState>) -> Result<ProfileOverview, String> {
    let config = state.config.lock().unwrap().clone();

    // Pro path: check server for profile
    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        if crate::keychain::get_api_key("noren-pro-token").is_some() {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.usenoren.ai");

            match fetch_server_profile_metadata(server_url).await {
                Ok(meta) if meta.has_profile => {
                    return Ok(ProfileOverview {
                        exists: true,
                        path: String::new(),
                        formats: meta.formats,
                        is_server: true,
                        voice_overview: meta.voice_overview,
                    });
                }
                Ok(meta) if !meta.has_profile && !config.debug_mode => {
                    return Ok(ProfileOverview {
                        exists: false,
                        path: String::new(),
                        formats: vec![],
                        is_server: true,
                        voice_overview: None,
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

    // Build voice overview from local files
    let voice_overview = if exists {
        build_local_voice_overview(dir)
    } else {
        None
    };

    Ok(ProfileOverview {
        exists,
        path: dir.to_string_lossy().to_string(),
        formats,
        is_server: false,
        voice_overview,
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

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    // Load local profile
    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    let qc_path = config.profile_dir.join("quality-check-results.md");
    let quality_report = std::fs::read_to_string(qc_path).ok();

    // Upload to server
    let upload_url = format!("{}/v1/profile/voice", server_url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "core_identity": core_identity,
        "contexts": contexts,
        "quality_report": quality_report,
    });
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .put(&upload_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&payload)
    })
    .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Server rejected profile upload: {}", text));
    }

    // Delete local profile
    let _ = std::fs::remove_dir_all(&config.profile_dir);

    Ok("Profile migrated to Noren servers".to_string())
}

/// Export server-side profile to a user-chosen location as Markdown.
#[tauri::command]
pub async fn export_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let export_url = format!("{}/v1/profile/voice/export", server_url.trim_end_matches('/'));
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&export_url)
            .header("Authorization", format!("Bearer {}", token))
    })
    .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Export failed: {}", text));
    }

    #[derive(Deserialize)]
    struct ExportResponse {
        core_identity: String,
        contexts: std::collections::HashMap<String, String>,
        #[allow(dead_code)]
        quality_report: Option<String>,
    }

    let export: ExportResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse export: {}", e))?;

    // Build Markdown
    let mut md = String::from("# Voice Profile\n\n## Core Identity\n\n");
    md.push_str(&export.core_identity);

    let mut formats: Vec<_> = export.contexts.iter().collect();
    formats.sort_by_key(|(k, _)| (*k).clone());
    for (fmt, content) in formats {
        let title = fmt
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string() + &fmt[1..])
            .unwrap_or_else(|| fmt.clone());
        md.push_str(&format!("\n\n---\n\n## Context: {}\n\n", title));
        md.push_str(content);
    }

    // Open native save dialog
    use tauri_plugin_dialog::DialogExt;

    let file_path = app
        .dialog()
        .file()
        .set_title("Export Voice Profile")
        .set_file_name("voice-profile.md")
        .add_filter("Markdown", &["md"])
        .blocking_save_file();

    let path = match file_path {
        Some(p) => p,
        None => return Err("Export cancelled".to_string()),
    };

    std::fs::write(path.as_path().unwrap(), md.as_bytes())
        .map_err(|e| format!("Failed to write file: {}", e))?;

    // Also save to internal profile directory for local BYOK use
    noren_engine::save_profile(
        &config.profile_dir,
        &export.core_identity,
        &export.contexts,
        &export.quality_report.unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    Ok(path.as_path().unwrap().to_string_lossy().to_string())
}

/// Build voice overview from local files (BYOK path).
fn build_local_voice_overview(profile_dir: &std::path::Path) -> Option<serde_json::Value> {
    let metadata_path = profile_dir.join("voice-metadata.json");
    let summary_path = profile_dir.join("voice-summary.txt");

    let metadata: Option<serde_json::Value> = std::fs::read_to_string(&metadata_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let summary: Option<String> = std::fs::read_to_string(&summary_path).ok();

    if metadata.is_none() && summary.is_none() {
        return None;
    }

    let vm = metadata.unwrap_or_default();

    // Strip exampleSentences from rhythm data
    fn strip_examples(rhythm: &serde_json::Value) -> serde_json::Value {
        if let Some(obj) = rhythm.as_object() {
            let mut cleaned = obj.clone();
            cleaned.remove("exampleSentences");
            serde_json::Value::Object(cleaned)
        } else {
            rhythm.clone()
        }
    }

    let baseline = vm.get("baselineRhythm").map(strip_examples);
    let format_rhythms = vm.get("formatRhythms").and_then(|fr| {
        fr.as_object().map(|obj| {
            let cleaned: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), strip_examples(v)))
                .collect();
            serde_json::Value::Object(cleaned)
        })
    });

    let overview = serde_json::json!({
        "summary": summary,
        "routing": vm.get("routing"),
        "counts": vm.get("counts"),
        "corpus": vm.get("corpus"),
        "baseline_rhythm": baseline,
        "format_rhythms": format_rhythms,
    });

    Some(overview)
}

/// Fetch profile metadata from the Noren server.
async fn fetch_server_profile_metadata(
    server_url: &str,
) -> Result<ServerProfileMetadata, String> {
    let url = format!("{}/v1/profile/voice/metadata", server_url.trim_end_matches('/'));
    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
    })
    .await?;

    if !resp.status().is_success() {
        return Err("Failed to fetch profile metadata".to_string());
    }

    resp.json::<ServerProfileMetadata>()
        .await
        .map_err(|e| e.to_string())
}
