// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accessibility;
mod auth_client;
mod clipboard;
mod commands;
mod hotkey;
mod keychain;
mod native_messaging;
mod tray;
mod window;

use std::sync::Mutex;

use tauri::Manager;

/// State for hotkey-captured text and source app tracking
pub struct ContextState {
    pub selected_text: Mutex<Option<String>>,
    pub source_pid: Mutex<Option<i32>>,
    pub source_app_name: Mutex<Option<String>>,
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
    // Get the saved source app info before hiding
    let source_pid = state.source_pid.lock().unwrap().take();
    let source_app_name = state.source_app_name.lock().unwrap().take();

    // Hide our popup
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.hide();
    }

    // IMPORTANT: Run injection on a background thread so the main thread can
    // process the window hide. Blocking the main thread here would prevent
    // hide() from completing, causing paste to fire into Noren instead of
    // the source app.
    std::thread::spawn(move || {
        if let Err(e) = clipboard::inject_text(&app, &text, source_pid, source_app_name) {
            eprintln!("[inject] error: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    window::show_main_window(&app);
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

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkey::handle_shortcut)
                .build(),
        )
        .manage(ContextState {
            selected_text: Mutex::new(None),
            source_pid: Mutex::new(None),
            source_app_name: Mutex::new(None),
        })
        .manage(AppState {
            config: Mutex::new(config),
            encryption_key,
        })
        .invoke_handler(tauri::generate_handler![
            get_context_text,
            inject_generated_text,
            show_main_window,
            check_permissions,
            request_permissions,
            commands::generate,
            commands::generate_stream,
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
            commands::start_extraction,
            commands::start_extraction_multi,
            commands::generate_comparison,
            commands::get_noren_pro_status,
            commands::noren_pro_login,
            commands::noren_pro_signup,
            commands::noren_pro_logout,
            commands::verify_email,
            commands::resend_otp,
            commands::resend_setup_email,
            commands::request_password_reset,
            commands::request_delete_account,
            commands::confirm_delete_account,
            commands::google_oauth_init,
            commands::google_oauth_poll,
            commands::get_noren_pro_usage,
            commands::set_inference_mode,
            commands::get_subscription_status,
            commands::create_checkout,
            commands::open_billing_portal,
            commands::redeem_coupon,
            commands::create_guest_checkout,
            commands::poll_guest_checkout,
            commands::restore_guest_purchase,
            commands::store_extraction_receipt,
            commands::has_extraction_receipt,
            commands::has_used_extraction,
            commands::mark_extraction_used,
            commands::store_pending_checkout,
            commands::get_pending_checkout,
            commands::clear_pending_checkout,
            commands::get_living_profile_status,
            commands::set_living_profile_enabled,
            commands::log_edit,
            commands::upload_edit_log,
            commands::refresh_living_profile,
            commands::get_profile_metadata,
            commands::rollback_profile,
            commands::get_refresh_history,
            commands::sync_profile_up,
            commands::sync_profile_down,
            commands::get_sync_status,
            commands::read_file_as_text,
            commands::migrate_profile_to_server,
            commands::export_profile,
            commands::update_hotkey,
            commands::list_ollama_models,
            commands::list_claude_models,
            commands::list_gemini_models,
            commands::list_openai_models,
            commands::list_custom_models,
            commands::get_thinking_settings,
            commands::set_thinking_settings,
            commands::chat_send,
            commands::chat_send_stream,
            commands::save_chat,
            commands::list_chats,
            commands::load_chat,
            commands::delete_chat,
            commands::sync_delete_chat,
            commands::sync_chats_from_server,
            commands::factory_reset,
            commands::fetch_announcements,
            commands::get_announcement_seen,
            commands::save_announcement_seen,
            commands::repurpose,
            commands::scrape_twitter,
            commands::scrape_blog,
            commands::scrape_reddit,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            // Request accessibility permission on first launch (opens System Settings if not granted)
            if !accessibility::check_accessibility_trusted(false) {
                accessibility::check_accessibility_trusted(true);
            }

            // Register Chrome native messaging host for keychain bridge
            native_messaging::register_chrome_host();

            tray::setup_tray(app.handle())?;
            {
                let state = app.state::<AppState>();
                let config = state.config.lock().unwrap();
                hotkey::register(app.handle(), &config.hotkey)?;
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Noren");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = &event {
            window::show_main_window(app_handle);
        }
    });
}
