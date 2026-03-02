use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;

/// Default TTL for cached prompts: 7 days
const DEFAULT_TTL_HOURS: u64 = 168;

/// Cached prompt envelope — encrypted content + metadata
#[derive(Serialize, Deserialize)]
struct CacheEnvelope {
    /// AES-256-GCM encrypted prompt content
    ciphertext: Vec<u8>,
    /// 12-byte nonce used for encryption
    nonce: Vec<u8>,
    /// Unix timestamp when this was cached
    cached_at: u64,
    /// TTL in hours
    ttl_hours: u64,
    /// Prompt version from server
    version: String,
}

/// Response from the prompt server
#[derive(Deserialize)]
pub struct PromptResponse {
    pub content: String,
    pub version: String,
    #[serde(default = "default_ttl")]
    pub ttl_hours: u64,
}

fn default_ttl() -> u64 {
    DEFAULT_TTL_HOURS
}

/// Get the enforcement prompt. Tries in order:
/// 1. Dev mode override (NOREN_DEV_PROMPT_PATH env var)
/// 2. Local encrypted cache
/// 3. Fetch from server
pub async fn get_enforcement_prompt(
    cache_dir: &Path,
    encryption_key: &[u8; 32],
    server_url: Option<&str>,
    auth_token: Option<&str>,
) -> Result<String, EngineError> {
    // 1. Dev mode: read from local filesystem (env var or well-known path)
    if let Ok(path) = std::env::var("NOREN_DEV_PROMPT_PATH") {
        return std::fs::read_to_string(&path).map_err(|e| {
            EngineError::PromptCache(format!("Failed to read dev prompt at {}: {}", path, e))
        });
    }
    let dev_path = default_dev_prompt_path();
    if dev_path.exists() {
        return std::fs::read_to_string(&dev_path).map_err(|e| {
            EngineError::PromptCache(format!(
                "Failed to read dev prompt at {}: {}",
                dev_path.display(),
                e
            ))
        });
    }

    // 2. Try loading from encrypted cache
    if let Some(content) = load_cached_prompt(cache_dir, encryption_key)? {
        return Ok(content);
    }

    // 3. Fetch from server and cache
    let server_url = match server_url {
        Some(url) => url,
        None => {
            // No server available — use built-in fallback prompt
            return Ok(BUILTIN_ENFORCEMENT_PROMPT.to_string());
        }
    };

    let auth_token = auth_token.ok_or_else(|| {
        EngineError::PromptCache("No auth token configured for prompt server.".to_string())
    })?;

    let response = fetch_enforcement_prompt(server_url, auth_token).await?;
    cache_prompt(&response.content, &response.version, response.ttl_hours, cache_dir, encryption_key)?;
    Ok(response.content)
}

/// Fetch the enforcement prompt from the server
pub async fn fetch_enforcement_prompt(
    server_url: &str,
    auth_token: &str,
) -> Result<PromptResponse, EngineError> {
    let url = format!("{}/v1/prompts/enforcement", server_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .map_err(|e| EngineError::PromptCache(format!("Failed to fetch prompt: {}", e)))?;

    if !resp.status().is_success() {
        return Err(EngineError::PromptCache(format!(
            "Server returned status {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }

    resp.json::<PromptResponse>()
        .await
        .map_err(|e| EngineError::PromptCache(format!("Failed to parse prompt response: {}", e)))
}

/// Encrypt and cache a prompt to disk
pub fn cache_prompt(
    content: &str,
    version: &str,
    ttl_hours: u64,
    cache_dir: &Path,
    encryption_key: &[u8; 32],
) -> Result<(), EngineError> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| EngineError::PromptCache(format!("Failed to create cache dir: {}", e)))?;

    let cipher = Aes256Gcm::new_from_slice(encryption_key)
        .map_err(|e| EngineError::PromptCache(format!("Invalid encryption key: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, content.as_bytes())
        .map_err(|e| EngineError::PromptCache(format!("Encryption failed: {}", e)))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let envelope = CacheEnvelope {
        ciphertext,
        nonce: nonce_bytes.to_vec(),
        cached_at: now,
        ttl_hours,
        version: version.to_string(),
    };

    let json = serde_json::to_vec(&envelope)?;
    let cache_path = cache_dir.join("enforcement.enc");
    std::fs::write(&cache_path, json)
        .map_err(|e| EngineError::PromptCache(format!("Failed to write cache: {}", e)))?;

    Ok(())
}

/// Load and decrypt a cached prompt. Returns None if cache is missing or expired.
pub fn load_cached_prompt(
    cache_dir: &Path,
    encryption_key: &[u8; 32],
) -> Result<Option<String>, EngineError> {
    let cache_path = cache_dir.join("enforcement.enc");

    if !cache_path.exists() {
        return Ok(None);
    }

    let data = std::fs::read(&cache_path)
        .map_err(|e| EngineError::PromptCache(format!("Failed to read cache: {}", e)))?;

    let envelope: CacheEnvelope = serde_json::from_slice(&data)
        .map_err(|e| EngineError::PromptCache(format!("Failed to parse cache: {}", e)))?;

    // Check TTL
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ttl_secs = envelope.ttl_hours * 3600;
    if now > envelope.cached_at + ttl_secs {
        // Cache expired
        return Ok(None);
    }

    // Decrypt
    let cipher = Aes256Gcm::new_from_slice(encryption_key)
        .map_err(|e| EngineError::PromptCache(format!("Invalid encryption key: {}", e)))?;

    if envelope.nonce.len() != 12 {
        return Err(EngineError::PromptCache("Invalid nonce length".to_string()));
    }
    let nonce = Nonce::from_slice(&envelope.nonce);

    let plaintext = cipher
        .decrypt(nonce, envelope.ciphertext.as_ref())
        .map_err(|e| EngineError::PromptCache(format!("Decryption failed: {}", e)))?;

    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|e| EngineError::PromptCache(format!("Invalid UTF-8 in cached prompt: {}", e)))
}

/// Generate a fresh 256-bit encryption key
pub fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

/// Get the default cache directory path
pub fn default_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".noren").join("cache")
}

/// Well-known dev prompt path: ~/.noren/dev-prompt.md
pub fn default_dev_prompt_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".noren").join("dev-prompt.md")
}

/// Built-in enforcement prompt for free/BYOK users.
pub const BUILTIN_ENFORCEMENT_PROMPT: &str = r#"### System Prompt

```
You are a writing assistant. Write {{FORMAT}} content in the voice described below.

{{CORE_IDENTITY}}

{{#if CONTEXT_LAYER}}
{{CONTEXT_LAYER}}
{{/if}}

{{USER_REQUEST}}

Write only the final text. No commentary.
```
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let key = generate_encryption_key();
        let content = "This is the enforcement prompt content with {{VARIABLES}} and stuff.";

        cache_prompt(content, "1.0.0", DEFAULT_TTL_HOURS, tmp.path(), &key).unwrap();

        let loaded = load_cached_prompt(tmp.path(), &key).unwrap();
        assert_eq!(loaded, Some(content.to_string()));
    }

    #[test]
    fn returns_none_for_missing_cache() {
        let tmp = TempDir::new().unwrap();
        let key = generate_encryption_key();

        let loaded = load_cached_prompt(tmp.path(), &key).unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn returns_none_for_expired_cache() {
        let tmp = TempDir::new().unwrap();
        let key = generate_encryption_key();

        // Manually create an expired envelope
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, b"expired content".as_ref()).unwrap();

        let past = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (200 * 3600); // 200 hours ago

        let envelope = CacheEnvelope {
            ciphertext,
            nonce: nonce_bytes.to_vec(),
            cached_at: past,
            ttl_hours: 168, // 7 days = 168 hours, but cached 200 hours ago
            version: "1.0.0".to_string(),
        };

        let json = serde_json::to_vec(&envelope).unwrap();
        std::fs::write(tmp.path().join("enforcement.enc"), json).unwrap();

        let loaded = load_cached_prompt(tmp.path(), &key).unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let tmp = TempDir::new().unwrap();
        let key1 = generate_encryption_key();
        let key2 = generate_encryption_key();

        cache_prompt("secret content", "1.0.0", DEFAULT_TTL_HOURS, tmp.path(), &key1).unwrap();

        let result = load_cached_prompt(tmp.path(), &key2);
        assert!(result.is_err());
    }

    #[test]
    fn dev_mode_override() {
        let tmp = TempDir::new().unwrap();
        let prompt_path = tmp.path().join("dev-prompt.md");
        std::fs::write(&prompt_path, "dev mode prompt content").unwrap();

        std::env::set_var("NOREN_DEV_PROMPT_PATH", prompt_path.to_str().unwrap());

        let key = generate_encryption_key();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(get_enforcement_prompt(
            tmp.path(),
            &key,
            None,
            None,
        ));

        std::env::remove_var("NOREN_DEV_PROMPT_PATH");

        assert_eq!(result.unwrap(), "dev mode prompt content");
    }
}
