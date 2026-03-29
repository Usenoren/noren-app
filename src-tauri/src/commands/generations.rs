use serde::{Deserialize, Serialize};
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
