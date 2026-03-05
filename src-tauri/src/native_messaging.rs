//! Registers the Chrome Native Messaging host manifest so the Noren extension
//! can communicate with the keychain host binary.

use std::fs;
use std::path::PathBuf;

/// The extension ID of the Noren Chrome extension.
/// Update this when publishing to the Chrome Web Store.
const EXTENSION_ID: &str = "ckioinifnoclbnkpcfndippfmfnebfek";

const HOST_NAME: &str = "ink.noren.keychain";

/// Registers the native messaging host manifest for Chrome.
/// Safe to call on every launch — overwrites the manifest with current values.
pub fn register_chrome_host() {
    if let Err(e) = do_register() {
        eprintln!("[native-messaging] failed to register host: {}", e);
    }
}

fn do_register() -> Result<(), String> {
    let host_binary = get_host_binary_path()?;
    let manifest_dir = get_chrome_native_hosts_dir()?;

    fs::create_dir_all(&manifest_dir)
        .map_err(|e| format!("create dir: {}", e))?;

    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "Noren Keychain Bridge — secure API key storage via macOS Keychain",
        "path": host_binary.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{}/", EXTENSION_ID)
        ]
    });

    let manifest_path = manifest_dir.join(format!("{}.json", HOST_NAME));
    let contents = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("serialize: {}", e))?;

    fs::write(&manifest_path, contents)
        .map_err(|e| format!("write {}: {}", manifest_path.display(), e))?;

    println!("[native-messaging] registered host at {}", manifest_path.display());
    Ok(())
}

/// Path to the noren-keychain-host binary.
/// Checks the .app bundle first (production), then cargo build output (development).
fn get_host_binary_path() -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("current_exe: {}", e))?
        .parent()
        .ok_or("no parent dir")?
        .to_path_buf();

    // Production: Tauri bundles sidecars with target-triple suffix in Contents/MacOS/
    let sidecar = exe_dir.join("noren-keychain-host-aarch64-apple-darwin");
    if sidecar.exists() {
        return Ok(sidecar);
    }

    // Production: also check without suffix
    let plain = exe_dir.join("noren-keychain-host");
    if plain.exists() {
        return Ok(plain);
    }

    Err(format!("noren-keychain-host not found in {}", exe_dir.display()))
}

fn get_chrome_native_hosts_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/Google/Chrome/NativeMessagingHosts"))
}
