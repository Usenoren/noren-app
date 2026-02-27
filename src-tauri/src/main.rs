// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accessibility;
mod clipboard;
mod commands;
mod hotkey;
mod keychain;
mod tray;
mod window;

use std::sync::Mutex;

use tauri::Manager;

/// State for hotkey-captured text and source app tracking
pub struct ContextState {
    pub selected_text: Mutex<Option<String>>,
    pub source_pid: Mutex<Option<i32>>,
}

/// Main app state: config + encryption key for prompt cache
pub struct AppState {
    pub config: Mutex<noren_engine::Config>,
    pub encryption_key: [u8; 32],
}

// --- Tauri commands ---

#[tauri::command]
fn get_context_text(state: tauri::State<ContextState>) -> Option<String> {
    state.selected_text.lock().unwrap().clone()
}

#[tauri::command]
fn inject_generated_text(
    app: tauri::AppHandle,
    state: tauri::State<ContextState>,
    text: String,
) -> Result<(), String> {
    // Get the saved source app PID before hiding
    let source_pid = state.source_pid.lock().unwrap().take();

    // Hide our popup
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }

    // osascript handles focus activation + paste via System Events
    clipboard::inject_text(&app, &text, source_pid)
}

#[tauri::command]
fn check_permissions() -> bool {
    accessibility::check_accessibility_trusted(false)
}

#[tauri::command]
fn request_permissions() -> bool {
    accessibility::check_accessibility_trusted(true)
}

// --- Initialization helpers ---

/// Load or create the encryption key for the prompt cache.
/// Priority: Keychain → file (legacy) → generate new → store in Keychain.
fn load_or_create_encryption_key() -> [u8; 32] {
    // 1. Try Keychain
    if let Some(key) = keychain::get_encryption_key() {
        return key;
    }

    // 2. Try legacy file (migrate to Keychain)
    let cache_dir = noren_engine::prompt_cache::default_cache_dir();
    let key_path = cache_dir.join("prompt-key");
    if let Ok(data) = std::fs::read(&key_path) {
        if data.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&data);
            // Migrate to Keychain
            let _ = keychain::store_encryption_key(&key);
            let _ = std::fs::remove_file(&key_path);
            return key;
        }
    }

    // 3. Generate new key and store in Keychain
    let key = noren_engine::prompt_cache::generate_encryption_key();
    let _ = keychain::store_encryption_key(&key);
    key
}

fn main() {
    let config = noren_engine::load_config(None);
    let encryption_key = load_or_create_encryption_key();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkey::handle_shortcut)
                .build(),
        )
        .manage(ContextState {
            selected_text: Mutex::new(None),
            source_pid: Mutex::new(None),
        })
        .manage(AppState {
            config: Mutex::new(config),
            encryption_key,
        })
        .invoke_handler(tauri::generate_handler![
            get_context_text,
            inject_generated_text,
            check_permissions,
            request_permissions,
            commands::generate,
            commands::list_formats,
            commands::get_config,
            commands::get_settings,
            commands::set_provider,
            commands::save_api_key,
            commands::remove_api_key,
            commands::update_model,
            commands::update_base_url,
            commands::test_connection,
            commands::get_profile_overview,
            commands::read_profile_content,
            commands::save_profile_edit,
            commands::run_extraction,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::setup_tray(app.handle())?;
            hotkey::register(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Noren");
}
