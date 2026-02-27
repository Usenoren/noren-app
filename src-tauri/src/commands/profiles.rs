use serde::Serialize;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
pub struct ProfileOverview {
    pub exists: bool,
    pub path: String,
    pub formats: Vec<String>,
}

#[tauri::command]
pub fn get_profile_overview(state: State<'_, AppState>) -> ProfileOverview {
    let config = state.config.lock().unwrap();
    let dir = &config.profile_dir;
    let exists = dir.join("core-identity.md").exists();
    let formats = if exists {
        noren_engine::list_formats(dir)
    } else {
        vec![]
    };
    ProfileOverview {
        exists,
        path: dir.to_string_lossy().to_string(),
        formats,
    }
}

#[tauri::command]
pub fn read_profile_content(
    state: State<'_, AppState>,
) -> Result<noren_engine::ProfileContent, String> {
    let config = state.config.lock().unwrap();
    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    // Read quality check if it exists
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

    // Save core identity
    std::fs::write(dir.join("core-identity.md"), &core_identity)
        .map_err(|e| format!("Failed to save core identity: {}", e))?;

    // Save context if provided
    if let (Some(fmt), Some(content)) = (context_format, context_content) {
        let contexts_dir = dir.join("contexts");
        let _ = std::fs::create_dir_all(&contexts_dir);
        std::fs::write(contexts_dir.join(format!("{}.md", fmt)), &content)
            .map_err(|e| format!("Failed to save context: {}", e))?;
    }

    Ok(())
}
