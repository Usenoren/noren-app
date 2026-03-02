use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{accessibility, ContextState};

const WINDOW_LABEL: &str = "main";
const WINDOW_WIDTH: f64 = 400.0;
const WINDOW_HEIGHT: f64 = 500.0;

/// Save the frontmost app's PID so inject can re-activate it later.
fn save_source_pid(app: &AppHandle) {
    if let Some(pid) = accessibility::get_frontmost_pid() {
        if let Some(state) = app.try_state::<ContextState>() {
            *state.source_pid.lock().unwrap() = Some(pid);
        }
    }
}

pub fn toggle_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
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
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        show_existing(&window);
    } else {
        create_popup(app);
    }
}

fn show_existing(window: &tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

fn create_popup(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::default())
        .title("Noren")
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .min_inner_size(320.0, 400.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .center();

    match builder.build() {
        Ok(_) => {}
        Err(e) => eprintln!("Failed to create popup window: {}", e),
    }
}
