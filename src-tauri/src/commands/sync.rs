use serde::Serialize;
use tauri::State;

use crate::{keychain, AppState};

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, AeadCore,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

#[derive(Serialize)]
pub struct SyncStatus {
    pub has_remote: bool,
    pub remote_version: Option<u64>,
    pub updated_at: Option<String>,
    pub local_checksum: String,
}

#[tauri::command]
pub async fn sync_profile_up(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    // Read local profile files
    let plaintext = read_profile_files(&config.profile_dir)?;
    if plaintext.is_empty() {
        return Err("No profile to sync".to_string());
    }

    // Encrypt with the app's encryption key
    let key = state.encryption_key;
    let (encrypted_data, nonce) = encrypt_data(&plaintext, &key)?;

    // Compute checksum of plaintext
    let checksum = sha256_hex(&plaintext);

    // Get current remote version to determine next version
    let client = reqwest::Client::new();
    let version = get_remote_version(&client, server_url, &auth_token)
        .await
        .unwrap_or(0) + 1;

    // Upload
    let resp: reqwest::Response = client
        .put(format!("{}/v1/sync/profile", server_url))
        .bearer_auth(&auth_token)
        .json(&serde_json::json!({
            "encrypted_data": encrypted_data,
            "nonce": nonce,
            "version": version,
            "checksum": checksum,
        }))
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Sync failed: {}", body));
    }

    Ok(format!("Synced v{}", version))
}

#[tauri::command]
pub async fn sync_profile_down(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .get(format!("{}/v1/sync/profile", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Download failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let encrypted_data = data["encrypted_data"]
        .as_str()
        .ok_or("Missing encrypted_data")?;
    let nonce = data["nonce"]
        .as_str()
        .ok_or("Missing nonce")?;
    let expected_checksum = data["checksum"]
        .as_str()
        .unwrap_or("");

    // Decrypt
    let key = state.encryption_key;
    let plaintext = decrypt_data(encrypted_data, nonce, &key)?;

    // Verify checksum
    let actual_checksum = sha256_hex(&plaintext);
    if !expected_checksum.is_empty() && actual_checksum != expected_checksum {
        return Err("Checksum mismatch — profile may be corrupted".to_string());
    }

    // Write to local profile directory
    write_profile_files(&config.profile_dir, &plaintext)?;

    let version = data["version"].as_u64().unwrap_or(0);
    Ok(format!("Downloaded v{}", version))
}

#[tauri::command]
pub async fn get_sync_status(
    state: State<'_, AppState>,
) -> Result<SyncStatus, String> {
    let config = state.config.lock().unwrap().clone();
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai");
    let auth_token = keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in")?;

    let client = reqwest::Client::new();
    let resp: reqwest::Response = client
        .get(format!("{}/v1/sync/status", server_url))
        .bearer_auth(&auth_token)
        .send()
        .await
        .map_err(|e| format!("Failed: {}", e))?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    // Compute local checksum
    let local_checksum = match read_profile_files(&config.profile_dir) {
        Ok(content) => sha256_hex(&content),
        Err(_) => String::new(),
    };

    Ok(SyncStatus {
        has_remote: data["has_remote"].as_bool().unwrap_or(false),
        remote_version: data["remote_version"].as_u64(),
        updated_at: data["updated_at"].as_str().map(|s| s.to_string()),
        local_checksum,
    })
}

// --- Crypto helpers ---

fn encrypt_data(plaintext: &[u8], key: &[u8; 32]) -> Result<(String, String), String> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok((BASE64.encode(&ciphertext), BASE64.encode(&nonce)))
}

fn decrypt_data(encrypted_b64: &str, nonce_b64: &str, key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let ciphertext = BASE64
        .decode(encrypted_b64)
        .map_err(|e| format!("Invalid base64: {}", e))?;
    let nonce_bytes = BASE64
        .decode(nonce_b64)
        .map_err(|e| format!("Invalid nonce base64: {}", e))?;

    let cipher = Aes256Gcm::new(key.into());
    let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Decryption failed — wrong key or corrupted data".to_string())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// --- Profile file helpers ---

fn read_profile_files(profile_dir: &std::path::Path) -> Result<Vec<u8>, String> {
    if !profile_dir.exists() {
        return Err("Profile directory not found".to_string());
    }

    // Pack all profile files into a single JSON blob
    let mut files: Vec<serde_json::Value> = Vec::new();

    fn collect_files(
        dir: &std::path::Path,
        base: &std::path::Path,
        files: &mut Vec<serde_json::Value>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(&path, base, files)?;
            } else if path.extension().map_or(false, |ext| ext == "md" || ext == "txt") {
                let rel_path = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string();
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                files.push(serde_json::json!({
                    "path": rel_path,
                    "content": content,
                }));
            }
        }
        Ok(())
    }

    collect_files(profile_dir, profile_dir, &mut files)?;

    if files.is_empty() {
        return Err("No profile files found".to_string());
    }

    serde_json::to_vec(&files).map_err(|e| format!("Serialization failed: {}", e))
}

fn write_profile_files(profile_dir: &std::path::Path, data: &[u8]) -> Result<(), String> {
    let files: Vec<serde_json::Value> =
        serde_json::from_slice(data).map_err(|e| format!("Invalid profile data: {}", e))?;

    // Ensure profile directory exists
    std::fs::create_dir_all(profile_dir).map_err(|e| e.to_string())?;

    for file in &files {
        let rel_path = file["path"]
            .as_str()
            .ok_or("Missing file path in sync data")?;
        let content = file["content"]
            .as_str()
            .ok_or("Missing file content in sync data")?;

        let full_path = profile_dir.join(rel_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // Atomic write: write to temp, then rename
        let temp_path = full_path.with_extension("tmp");
        std::fs::write(&temp_path, content).map_err(|e| e.to_string())?;
        std::fs::rename(&temp_path, &full_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn get_remote_version(
    client: &reqwest::Client,
    server_url: &str,
    auth_token: &str,
) -> Result<u64, String> {
    let resp: reqwest::Response = client
        .get(format!("{}/v1/sync/status", server_url))
        .bearer_auth(auth_token)
        .send()
        .await
        .map_err(|e| format!("Failed: {}", e))?;

    if !resp.status().is_success() {
        return Err("Failed to get remote version".to_string());
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(data["remote_version"].as_u64().unwrap_or(0))
}
