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
            *state.source_app_name.lock().unwrap() = app_name.clone();
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

/// Parse a hotkey string like "Meta+KeyK" or "Meta+Shift+KeyN" into a Shortcut.
pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return Err("Empty hotkey string".to_string());
    }

    let mut modifiers = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in &parts {
        match part.trim() {
            // Modifiers
            "Meta" | "Super" | "Cmd" | "Command" => modifiers |= Modifiers::META,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" | "Option" => modifiers |= Modifiers::ALT,
            "Ctrl" | "Control" => modifiers |= Modifiers::CONTROL,
            // Key codes
            k => {
                if code.is_some() {
                    return Err(format!("Multiple key codes found: {}", k));
                }
                code = Some(parse_code(k)?);
            }
        }
    }

    let code = code.ok_or("No key code found in hotkey string")?;
    if modifiers.is_empty() {
        return Err("At least one modifier key (Cmd, Ctrl, Alt, Shift) is required".to_string());
    }

    Ok(Shortcut::new(Some(modifiers), code))
}

fn parse_code(s: &str) -> Result<Code, String> {
    match s {
        // Letters
        "KeyA" => Ok(Code::KeyA), "KeyB" => Ok(Code::KeyB), "KeyC" => Ok(Code::KeyC),
        "KeyD" => Ok(Code::KeyD), "KeyE" => Ok(Code::KeyE), "KeyF" => Ok(Code::KeyF),
        "KeyG" => Ok(Code::KeyG), "KeyH" => Ok(Code::KeyH), "KeyI" => Ok(Code::KeyI),
        "KeyJ" => Ok(Code::KeyJ), "KeyK" => Ok(Code::KeyK), "KeyL" => Ok(Code::KeyL),
        "KeyM" => Ok(Code::KeyM), "KeyN" => Ok(Code::KeyN), "KeyO" => Ok(Code::KeyO),
        "KeyP" => Ok(Code::KeyP), "KeyQ" => Ok(Code::KeyQ), "KeyR" => Ok(Code::KeyR),
        "KeyS" => Ok(Code::KeyS), "KeyT" => Ok(Code::KeyT), "KeyU" => Ok(Code::KeyU),
        "KeyV" => Ok(Code::KeyV), "KeyW" => Ok(Code::KeyW), "KeyX" => Ok(Code::KeyX),
        "KeyY" => Ok(Code::KeyY), "KeyZ" => Ok(Code::KeyZ),
        // Digits
        "Digit0" => Ok(Code::Digit0), "Digit1" => Ok(Code::Digit1),
        "Digit2" => Ok(Code::Digit2), "Digit3" => Ok(Code::Digit3),
        "Digit4" => Ok(Code::Digit4), "Digit5" => Ok(Code::Digit5),
        "Digit6" => Ok(Code::Digit6), "Digit7" => Ok(Code::Digit7),
        "Digit8" => Ok(Code::Digit8), "Digit9" => Ok(Code::Digit9),
        // Special keys
        "Space" => Ok(Code::Space),
        "Comma" => Ok(Code::Comma),
        "Period" => Ok(Code::Period),
        "Slash" => Ok(Code::Slash),
        "Backslash" => Ok(Code::Backslash),
        "BracketLeft" => Ok(Code::BracketLeft),
        "BracketRight" => Ok(Code::BracketRight),
        "Semicolon" => Ok(Code::Semicolon),
        "Quote" => Ok(Code::Quote),
        "Backquote" => Ok(Code::Backquote),
        "Minus" => Ok(Code::Minus),
        "Equal" => Ok(Code::Equal),
        "Enter" => Ok(Code::Enter),
        "Escape" => Ok(Code::Escape),
        "Backspace" => Ok(Code::Backspace),
        "Tab" => Ok(Code::Tab),
        // Function keys
        "F1" => Ok(Code::F1), "F2" => Ok(Code::F2), "F3" => Ok(Code::F3),
        "F4" => Ok(Code::F4), "F5" => Ok(Code::F5), "F6" => Ok(Code::F6),
        "F7" => Ok(Code::F7), "F8" => Ok(Code::F8), "F9" => Ok(Code::F9),
        "F10" => Ok(Code::F10), "F11" => Ok(Code::F11), "F12" => Ok(Code::F12),
        _ => Err(format!("Unknown key code: {}", s)),
    }
}

/// Register the global shortcut from a hotkey string.
pub fn register(app: &AppHandle, hotkey_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut = parse_shortcut(hotkey_str)?;
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

/// Unregister all shortcuts and register a new one.
pub fn re_register(app: &AppHandle, new_hotkey_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(new_hotkey_str)?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("Failed to unregister shortcuts: {}", e))?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;
    Ok(())
}
