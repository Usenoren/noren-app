//! macOS Keychain integration for secure storage of API keys and encryption keys.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const SERVICE: &str = "com.noren.app";

// --- API Keys ---

pub fn store_api_key(provider: &str, key: &str) -> Result<(), String> {
    let account = format!("api-key-{}", provider);
    set_generic_password(SERVICE, &account, key.as_bytes())
        .map_err(|e| format!("Failed to store API key: {}", e))
}

pub fn get_api_key(provider: &str) -> Option<String> {
    let account = format!("api-key-{}", provider);
    get_generic_password(SERVICE, &account)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn delete_api_key(provider: &str) -> Result<(), String> {
    let account = format!("api-key-{}", provider);
    delete_generic_password(SERVICE, &account)
        .map_err(|e| format!("Failed to delete API key: {}", e))
}

// --- Prompt encryption key ---

const ENCRYPTION_KEY_ACCOUNT: &str = "prompt-encryption-key";

pub fn store_encryption_key(key: &[u8; 32]) -> Result<(), String> {
    set_generic_password(SERVICE, ENCRYPTION_KEY_ACCOUNT, key)
        .map_err(|e| format!("Failed to store encryption key: {}", e))
}

pub fn get_encryption_key() -> Option<[u8; 32]> {
    get_generic_password(SERVICE, ENCRYPTION_KEY_ACCOUNT)
        .ok()
        .and_then(|bytes| {
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                Some(key)
            } else {
                None
            }
        })
}
