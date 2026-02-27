//! Global hotkey registration and handling.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

use crate::{accessibility, clipboard, window, ContextState};

/// Handler called when any registered global shortcut is pressed.
pub fn handle_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        // Save the frontmost app's PID so we can re-activate it on inject
        let source_pid = accessibility::get_frontmost_pid();

        // Detect the frontmost app name and its format
        let app_name = accessibility::get_frontmost_app_name();
        let detected_format = app_name
            .as_deref()
            .and_then(accessibility::detect_format);

        // Capture selected text before showing our window (which steals focus)
        let text = clipboard::get_selected_text(&app);

        // Store in app state
        if let Some(state) = app.try_state::<ContextState>() {
            *state.selected_text.lock().unwrap() = text.clone();
            *state.source_pid.lock().unwrap() = source_pid;
        }

        // Emit context to frontend
        let _ = app.emit("context-text", text.unwrap_or_default());

        // Emit detected app info
        if let Some(name) = &app_name {
            let _ = app.emit("detected-app", serde_json::json!({
                "name": name,
                "format": detected_format,
            }));
        }

        // Show popup
        window::show_popup(&app);
    });
}

/// Register the global Cmd+K shortcut.
pub fn register(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut = Shortcut::new(Some(Modifiers::META), Code::KeyK);
    app.global_shortcut().register(shortcut)?;
    Ok(())
}
