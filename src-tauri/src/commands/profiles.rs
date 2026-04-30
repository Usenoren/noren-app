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

#[derive(Serialize)]
pub struct LocalProfileCleanupResult {
    pub removed_profile: bool,
    pub removed_quality_report: bool,
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
                    return Ok(ProfileOverview {
                        exists: false,
                        path: String::new(),
                        formats: vec![],
                        is_server: true,
                        voice_overview: None,
                    });
                }
            }
        }

        return Ok(ProfileOverview {
            exists: false,
            path: String::new(),
            formats: vec![],
            is_server: true,
            voice_overview: None,
        });
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

    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        return Err("Profile is stored on Noren servers. Use Export to download.".to_string());
    }

    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    Ok(noren_engine::ProfileContent {
        core_identity,
        contexts,
        quality_check: None,
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
    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        return Err("Local profile edits are only available in BYOK mode.".to_string());
    }

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

#[tauri::command]
pub fn cleanup_local_profile_storage(
    state: State<'_, AppState>,
    can_export: Option<bool>,
) -> Result<LocalProfileCleanupResult, String> {
    let config = state.config.lock().unwrap().clone();
    let remove_profile =
        config.inference_mode == noren_engine::InferenceMode::NorenPro && can_export == Some(false);

    let mut removed_profile = false;
    let mut removed_quality_report = false;

    let dirs = cleanup_candidate_dirs(&config.profile_dir);
    for dir in dirs {
        let result = cleanup_profile_dir(&dir, remove_profile)?;
        removed_profile |= result.removed_profile;
        removed_quality_report |= result.removed_quality_report;
    }

    Ok(LocalProfileCleanupResult {
        removed_profile,
        removed_quality_report,
    })
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

    let export_url = format!(
        "{}/v1/profile/voice/export",
        server_url.trim_end_matches('/')
    );
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

    noren_engine::save_profile_for_byok_seed(
        &config.profile_dir,
        &export.core_identity,
        &export.contexts,
    )
    .map_err(|e| e.to_string())?;

    Ok(path.as_path().unwrap().to_string_lossy().to_string())
}

/// Apply a natural language instruction to modify the voice profile. Pro only.
#[derive(Serialize, Deserialize)]
pub struct GuidedEditResponse {
    pub edited: bool,
    pub section: String,
    pub original: String,
    pub updated: String,
    pub voice_summary: Option<String>,
    pub message: String,
}

#[tauri::command]
pub async fn guided_profile_edit(
    state: State<'_, AppState>,
    instruction: String,
    format: Option<String>,
) -> Result<GuidedEditResponse, String> {
    let config = state.config.lock().unwrap().clone();

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");

    let edit_url = format!("{}/v1/profile/voice/edit", server_url.trim_end_matches('/'));
    let mut payload = serde_json::json!({ "instruction": instruction });
    if let Some(fmt) = format {
        payload["format"] = serde_json::Value::String(fmt);
    }

    let resp = crate::auth_client::authed_request(server_url, |client, token| {
        client
            .post(&edit_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&payload)
    })
    .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Edit failed: {}", text));
    }

    resp.json::<GuidedEditResponse>()
        .await
        .map_err(|e| format!("Failed to parse edit response: {}", e))
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
async fn fetch_server_profile_metadata(server_url: &str) -> Result<ServerProfileMetadata, String> {
    let url = format!(
        "{}/v1/profile/voice/metadata",
        server_url.trim_end_matches('/')
    );
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

fn cleanup_candidate_dirs(profile_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut dirs = vec![profile_dir.to_path_buf()];
    if let Some(parent) = profile_dir.parent() {
        let legacy = parent.join("profile_dir");
        if legacy != profile_dir {
            dirs.push(legacy);
        }
    }
    dirs
}

fn cleanup_profile_dir(
    profile_dir: &std::path::Path,
    remove_profile: bool,
) -> Result<LocalProfileCleanupResult, String> {
    let quality_path = profile_dir.join("quality-check-results.md");
    let mut removed_quality_report = false;
    if quality_path.exists() {
        std::fs::remove_file(&quality_path)
            .map_err(|e| format!("Failed to remove quality report: {}", e))?;
        removed_quality_report = true;
    }

    let mut removed_profile = false;
    if remove_profile {
        for path in [
            profile_dir.join("core-identity.md"),
            profile_dir.join("calibration.json"),
            profile_dir.join("voice-metadata.json"),
            profile_dir.join("voice-summary.txt"),
        ] {
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove profile file: {}", e))?;
                removed_profile = true;
            }
        }

        let contexts_dir = profile_dir.join("contexts");
        if contexts_dir.exists() {
            std::fs::remove_dir_all(&contexts_dir)
                .map_err(|e| format!("Failed to remove profile contexts: {}", e))?;
            removed_profile = true;
        }

        if profile_dir
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir(profile_dir);
        }
    }

    Ok(LocalProfileCleanupResult {
        removed_profile,
        removed_quality_report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_profile_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noren-profile-cleanup-{}-{}", name, suffix))
    }

    #[test]
    fn cleanup_removes_quality_report_without_profile_content() {
        let dir = temp_profile_dir("quality");
        std::fs::create_dir_all(dir.join("contexts")).unwrap();
        std::fs::write(dir.join("core-identity.md"), "core").unwrap();
        std::fs::write(dir.join("contexts/email.md"), "email").unwrap();
        std::fs::write(dir.join("quality-check-results.md"), "quality").unwrap();

        let result = cleanup_profile_dir(&dir, false).unwrap();
        assert!(result.removed_quality_report);
        assert!(!result.removed_profile);
        assert!(dir.join("core-identity.md").exists());
        assert!(dir.join("contexts/email.md").exists());
        assert!(!dir.join("quality-check-results.md").exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_removes_profile_content_when_requested() {
        let dir = temp_profile_dir("profile");
        std::fs::create_dir_all(dir.join("contexts")).unwrap();
        std::fs::write(dir.join("core-identity.md"), "core").unwrap();
        std::fs::write(dir.join("contexts/email.md"), "email").unwrap();
        std::fs::write(dir.join("voice-metadata.json"), "{}").unwrap();

        let result = cleanup_profile_dir(&dir, true).unwrap();
        assert!(result.removed_profile);
        assert!(!dir.join("core-identity.md").exists());
        assert!(!dir.join("contexts").exists());
        assert!(!dir.join("voice-metadata.json").exists());

        let _ = std::fs::remove_dir_all(dir);
    }
}
