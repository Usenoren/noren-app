use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
#[allow(unused_imports)]
use cocoa::foundation::NSString as _;

use crate::{accessibility, ContextState};

// --- Popup (Cmd+K quick access) ---
const POPUP_LABEL: &str = "popup";
const POPUP_WIDTH: f64 = 440.0;
const POPUP_HEIGHT: f64 = 480.0;

// --- Main app window ---
const MAIN_LABEL: &str = "main-app";
const MAIN_WIDTH: f64 = 900.0;
const MAIN_HEIGHT: f64 = 650.0;

/// Save the frontmost app's PID so inject can re-activate it later.
fn save_source_pid(app: &AppHandle) {
    if let Some(pid) = accessibility::get_frontmost_pid() {
        if let Some(state) = app.try_state::<ContextState>() {
            *state.source_pid.lock().unwrap() = Some(pid);
        }
    }
}

// ── Popup ──────────────────────────────────────────────────

pub fn toggle_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            save_source_pid(app);
            show_existing(&window);
        }
    } else {
        save_source_pid(app);
        create_popup(app);
    }
}

pub fn show_popup(app: &AppHandle) {
    // Don't call save_source_pid here — the hotkey handler already captured it.
    // Calling it again would overwrite the real source PID with Noren's own PID.
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        show_existing(&window);
    } else {
        create_popup(app);
    }
}

fn create_popup(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::default())
        .title("Noren")
        .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
        .min_inner_size(POPUP_WIDTH, POPUP_HEIGHT)
        .max_inner_size(POPUP_WIDTH, POPUP_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .center();

    match builder.build() {
        Ok(window) => {
            apply_macos_transparency(&window);

            // Hide popup when it loses focus (debounced to avoid
            // swallowing clicks on the window's own close button)
            let w = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    let w2 = w.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if !w2.is_focused().unwrap_or(true) {
                            let _ = w2.hide();
                        }
                    });
                }
            });
        }
        Err(e) => eprintln!("Failed to create popup window: {}", e),
    }
}

/// Make the native NSWindow + WKWebView fully transparent so CSS border-radius shows through.
#[allow(deprecated)]
fn apply_macos_transparency(window: &tauri::WebviewWindow) {
    use cocoa::base::{id, NO};
    use objc::{class, msg_send, sel, sel_impl};

    let _ = window.with_webview(|webview| {
        unsafe {
            let wk: id = webview.inner() as id;

            // Make WKWebView background transparent (private API)
            let no_val: id = msg_send![class!(NSNumber), numberWithBool: NO];
            let key: id = cocoa::foundation::NSString::alloc(cocoa::base::nil)
                .init_str("drawsBackground");
            let _: () = msg_send![wk, setValue: no_val forKey: key];

            // Make the NSWindow background transparent
            let ns_window: id = msg_send![wk, window];
            let _: () = msg_send![ns_window, setOpaque: NO];
            let clear: id = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![ns_window, setBackgroundColor: clear];
        }
    });
}

// ── Main app window ────────────────────────────────────────

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        show_existing(&window);
    } else {
        create_main_window(app);
    }
}

fn create_main_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::default())
        .title("Noren")
        .inner_size(MAIN_WIDTH, MAIN_HEIGHT)
        .min_inner_size(600.0, 450.0)
        .decorations(true)
        .visible(true)
        .center();

    match builder.build() {
        Ok(window) => {
            // Hide instead of closing so the window can be re-shown from dock/tray
            let w = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = w.hide();
                }
            });
        }
        Err(e) => eprintln!("Failed to create main window: {}", e),
    }
}

// ── Shared ─────────────────────────────────────────────────

fn show_existing(window: &tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}
