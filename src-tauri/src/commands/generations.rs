use serde::{Deserialize, Serialize};
use tauri::State;
use crate::AppState;
use super::generate::GenerateResult;

// --- Types ---

#[derive(Serialize, Deserialize, Clone)]
pub struct Generation {
    pub id: String,
    pub timestamp: String,
    pub format: String,
    pub prompt: String,
    pub mode: String,
    pub output: GenerateResult,
    pub edits: Vec<GenerationEdit>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GenerationEdit {
    pub timestamp: String,
    pub instruction: String,
    pub before_text: String,
    pub after_text: String,
}

#[derive(Serialize)]
pub struct GenerationSummary {
    pub id: String,
    pub timestamp: String,
    pub format: String,
    pub prompt: String,
    pub mode: String,
    pub token_count: u64,
    pub is_edited: bool,
}

// --- Helpers ---

fn generations_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = std::path::PathBuf::from(home)
        .join(".noren")
        .join("generations");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn validate_generation_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("Invalid generation ID".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid generation ID".to_string());
    }
    Ok(())
}

// --- Commands ---

#[tauri::command]
pub fn save_generation(generation: Generation) -> Result<(), String> {
    validate_generation_id(&generation.id)?;
    let dir = generations_dir();
    let path = dir.join(format!("{}.json", generation.id));
    let json = serde_json::to_string_pretty(&generation)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to save generation: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn list_generations() -> Result<Vec<GenerationSummary>, String> {
    let dir = generations_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut summaries = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(gen) = serde_json::from_str::<Generation>(&content) {
                    summaries.push(GenerationSummary {
                        id: gen.id,
                        timestamp: gen.timestamp,
                        format: gen.format,
                        prompt: gen.prompt,
                        mode: gen.mode,
                        token_count: gen.output.input_tokens + gen.output.output_tokens,
                        is_edited: !gen.edits.is_empty(),
                    });
                }
            }
        }
    }

    // Most recent first
    summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(summaries)
}

#[tauri::command]
pub fn load_generation(id: String) -> Result<Generation, String> {
    validate_generation_id(&id)?;
    let path = generations_dir().join(format!("{}.json", id));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read generation: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse generation: {}", e))
}

#[tauri::command]
pub fn load_latest_generation() -> Result<Option<Generation>, String> {
    let dir = generations_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };

    // Find the most recently modified JSON file
    let latest = entries
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));

    match latest {
        Some(entry) => {
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| format!("Failed to read: {}", e))?;
            let gen = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse: {}", e))?;
            Ok(Some(gen))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn delete_generation(id: String) -> Result<(), String> {
    validate_generation_id(&id)?;
    let path = generations_dir().join(format!("{}.json", id));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete generation: {}", e))?;
    }
    Ok(())
}

/// Sync generations from server — downloads new/updated generations.
#[tauri::command]
pub async fn sync_generations_from_server(state: State<'_, AppState>) -> Result<u32, String> {
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode != noren_engine::InferenceMode::NorenPro {
        return Ok(0);
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .trim_end_matches('/')
        .to_string();

    if crate::keychain::get_api_key("noren-pro-token").is_none() {
        return Ok(0);
    }

    let manifest_url = format!("{}/v1/sync/generations/manifest", server_url);
    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client.get(&manifest_url).bearer_auth(token)
    })
    .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(0),
    };

    let manifest: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let generations = manifest["generations"].as_array().ok_or("Invalid manifest")?;

    let dir = generations_dir();
    let mut synced: u32 = 0;

    for entry in generations {
        let gen_id = entry["generation_id"].as_str().unwrap_or_default();
        if gen_id.is_empty() || validate_generation_id(gen_id).is_err() {
            continue;
        }

        let is_deleted = entry["is_deleted"].as_bool().unwrap_or(false);
        let local_path = dir.join(format!("{}.json", gen_id));

        if is_deleted {
            let _ = std::fs::remove_file(&local_path);
            continue;
        }

        if local_path.exists() {
            continue; // local exists, skip
        }

        let dl_url = format!("{}/v1/sync/generations/{}", server_url, gen_id);
        let dl_resp = crate::auth_client::authed_request(&server_url, |client, token| {
            client.get(&dl_url).bearer_auth(token)
        })
        .await;

        if let Ok(resp) = dl_resp {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    let gen = Generation {
                        id: gen_id.to_string(),
                        timestamp: data["created_at"].as_str().unwrap_or_default().to_string(),
                        format: data["format"].as_str().unwrap_or("general").to_string(),
                        prompt: data["prompt"].as_str().unwrap_or_default().to_string(),
                        mode: data["mode"].as_str().unwrap_or("generate").to_string(),
                        output: GenerateResult {
                            text: data["output"].as_str().unwrap_or_default().to_string(),
                            input_tokens: data["input_tokens"].as_u64().unwrap_or(0),
                            output_tokens: data["output_tokens"].as_u64().unwrap_or(0),
                            voice_check: None,
                            routed_model: None,
                            route_reason: None,
                        },
                        edits: data["edits"]
                            .as_array()
                            .map(|arr| arr.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect())
                            .unwrap_or_default(),
                    };

                    if let Ok(json) = serde_json::to_string_pretty(&gen) {
                        let _ = std::fs::write(local_path, json);
                        synced += 1;
                    }
                }
            }
        }
    }

    Ok(synced)
}

/// Sync generation edits to server.
#[tauri::command]
pub async fn sync_generation_edits(
    state: State<'_, AppState>,
    generation_id: String,
    edits: Vec<GenerationEdit>,
    edit_count: u32,
    was_edited: bool,
    time_to_first_edit: Option<u32>,
    output: Option<String>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode != noren_engine::InferenceMode::NorenPro {
        return Ok(());
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .trim_end_matches('/')
        .to_string();

    if crate::keychain::get_api_key("noren-pro-token").is_none() {
        return Ok(());
    }

    let url = format!("{}/v1/sync/generations/{}", server_url, generation_id);
    let body = serde_json::json!({
        "edits": edits,
        "edit_count": edit_count,
        "was_edited": was_edited,
        "time_to_first_edit": time_to_first_edit,
        "output": output,
    });

    let _ = crate::auth_client::authed_request(&server_url, |client, token| {
        client.patch(&url).bearer_auth(token).json(&body)
    })
    .await;

    Ok(())
}
