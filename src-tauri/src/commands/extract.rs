use std::sync::Arc;

use noren_engine::extraction::client::{ExtractionProgress, ServerExtractionClient};
use noren_engine::ExtractionClient;
use tauri::Emitter;

use crate::AppState;

const DEFAULT_SERVER_URL: &str = "http://localhost:8000";

#[tauri::command]
pub async fn run_extraction(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    samples: String,
    format: String,
) -> Result<String, String> {
    let server_url = {
        let config = state.config.lock().unwrap();
        config
            .server_url
            .clone()
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string())
    };

    // Get or create auth token
    let auth_token = get_or_create_auth_token(&server_url).await?;

    // Create server client with progress callback
    let app_handle = app.clone();
    let client = ServerExtractionClient::new(server_url, auth_token).with_progress(Box::new(
        move |progress: ExtractionProgress| {
            let _ = app_handle.emit("extraction-progress", &progress);
        },
    ));

    // Run extraction
    let result = client
        .extract(&samples, &format)
        .await
        .map_err(|e| e.to_string())?;

    if result.stored_server_side {
        Ok("Extraction complete — profile stored on Noren servers".to_string())
    } else {
        // Save profile to disk (BYOK path)
        let profile_dir = {
            let config = state.config.lock().unwrap();
            config.profile_dir.clone()
        };

        noren_engine::save_profile(
            &profile_dir,
            &result.core_identity,
            &result.contexts,
            &result.quality_check,
        )
        .map_err(|e| e.to_string())?;

        Ok("Extraction complete — profile saved".to_string())
    }
}

/// Get auth token — use Noren Pro token if logged in, otherwise fall back to device registration.
async fn get_or_create_auth_token(server_url: &str) -> Result<String, String> {
    // Prefer Noren Pro token (user signed in via account)
    if let Some(token) = crate::keychain::get_api_key("noren-pro-token") {
        return Ok(token);
    }

    // Fall back to existing device token
    if let Some(token) = crate::keychain::get_api_key("noren-server-token") {
        return Ok(token);
    }

    // Auto-register with device ID
    let device_id = format!("device-{}", whoami());
    let email = format!("{}@device.noren.ink", device_id);
    let password = format!("noren-device-{}", device_id);

    // Try login first, then register
    let token = match ServerExtractionClient::login(server_url, &email, &password).await {
        Ok(token) => token,
        Err(_) => ServerExtractionClient::register(server_url, &email, &password)
            .await
            .map_err(|e| format!("Failed to authenticate with server: {}", e))?,
    };

    // Store in keychain
    let _ = crate::keychain::store_api_key("noren-server-token", &token);

    Ok(token)
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Start extraction and return immediately (for async UI).
/// Progress is emitted via "extraction-progress" events.
#[tauri::command]
pub async fn start_extraction(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    samples: String,
    format: String,
) -> Result<(), String> {
    let server_url = {
        let config = state.config.lock().unwrap();
        config
            .server_url
            .clone()
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string())
    };

    let profile_dir = {
        let config = state.config.lock().unwrap();
        config.profile_dir.clone()
    };

    let auth_token = get_or_create_auth_token(&server_url).await?;

    let app_handle = Arc::new(app.clone());
    let app_for_progress = app_handle.clone();
    let app_for_done = app_handle.clone();

    // Spawn extraction in background
    tokio::spawn(async move {
        let client =
            ServerExtractionClient::new(server_url, auth_token).with_progress(Box::new(
                move |progress: ExtractionProgress| {
                    let _ = app_for_progress.emit("extraction-progress", &progress);
                },
            ));

        match client.extract(&samples, &format).await {
            Ok(result) => {
                if result.stored_server_side {
                    // Pro path: profile stored on server — skip local save
                    let _ = app_for_done.emit(
                        "extraction-progress",
                        &ExtractionProgress {
                            status: "stored_server".to_string(),
                            progress: 100,
                            error: None,
                        },
                    );
                } else {
                    // BYOK path: save profile locally
                    match noren_engine::save_profile(
                        &profile_dir,
                        &result.core_identity,
                        &result.contexts,
                        &result.quality_check,
                    ) {
                        Ok(_) => {
                            let _ = app_for_done.emit(
                                "extraction-progress",
                                &ExtractionProgress {
                                    status: "saved".to_string(),
                                    progress: 100,
                                    error: None,
                                },
                            );
                        }
                        Err(e) => {
                            let _ = app_for_done.emit(
                                "extraction-progress",
                                &ExtractionProgress {
                                    status: "failed".to_string(),
                                    progress: 0,
                                    error: Some(format!("Failed to save profile: {}", e)),
                                },
                            );
                        }
                    }
                }
            }
            Err(e) => {
                let _ = app_for_done.emit(
                    "extraction-progress",
                    &ExtractionProgress {
                        status: "failed".to_string(),
                        progress: 0,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    });

    Ok(())
}
