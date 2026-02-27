//! Clipboard management and text injection.

use std::ffi::c_void;
use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::accessibility;

type CGEventRef = *const c_void;
type CGEventSourceRef = *const c_void;

const K_CG_HID_EVENT_TAP: u32 = 0;
const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM: u32 = 1;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const K_VK_ANSI_C: u16 = 0x08;
const K_VK_ANSI_V: u16 = 0x09;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state_id: u32) -> CGEventSourceRef;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

/// Simulate a key press with modifier flags using CGEvent.
fn simulate_key_combo(key: u16, flags: u64) {
    unsafe {
        let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM);

        // Key down
        let key_down = CGEventCreateKeyboardEvent(source, key, true);
        CGEventSetFlags(key_down, flags);
        CGEventPost(K_CG_HID_EVENT_TAP, key_down);

        // Key up
        let key_up = CGEventCreateKeyboardEvent(source, key, false);
        CGEventSetFlags(key_up, flags);
        CGEventPost(K_CG_HID_EVENT_TAP, key_up);

        // Cleanup
        if !key_down.is_null() {
            CFRelease(key_down);
        }
        if !key_up.is_null() {
            CFRelease(key_up);
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

/// Get selected text: try AX API first, fall back to clipboard Cmd+C.
pub fn get_selected_text(app: &AppHandle) -> Option<String> {
    // Try AX API first (fast, no side effects)
    if let Some(text) = accessibility::get_selected_text_ax() {
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Fallback: use clipboard + Cmd+C
    get_selected_text_clipboard(app)
}

/// Fallback method: simulate Cmd+C and read from clipboard.
fn get_selected_text_clipboard(app: &AppHandle) -> Option<String> {
    // Save current clipboard content
    let saved = app.clipboard().read_text().ok();

    // Clear clipboard so we can detect if Cmd+C wrote something
    let _ = app.clipboard().write_text("");

    // Simulate Cmd+C
    simulate_key_combo(K_VK_ANSI_C, K_CG_EVENT_FLAG_MASK_COMMAND);

    // Wait for clipboard to update
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Read new clipboard content
    let new_content = app.clipboard().read_text().ok();

    // Restore original clipboard
    if let Some(ref saved_text) = saved {
        let _ = app.clipboard().write_text(saved_text.as_str());
    }

    // Return the captured text if clipboard changed from empty
    match new_content {
        Some(text) if !text.is_empty() => Some(text),
        _ => None,
    }
}

/// Inject text into the frontmost application.
/// 1. Writes text to clipboard
/// 2. Activates the source app via NSRunningApplication
/// 3. Uses osascript + System Events to simulate Cmd+V
pub fn inject_text(app: &AppHandle, text: &str, source_pid: Option<i32>) -> Result<(), String> {
    // Write generated text to clipboard
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Failed to write to clipboard: {}", e))?;

    // Activate the source app first via native API (no permissions needed)
    if let Some(pid) = source_pid {
        accessibility::activate_app(pid);
        // Give macOS time to complete the activation
        std::thread::sleep(std::time::Duration::from_millis(300));
    } else {
        // No source PID — wait for macOS to activate previous app after our window hides
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // Try osascript to simulate Cmd+V
    let result = Command::new("/usr/bin/osascript")
        .args(["-e", r#"tell application "System Events" to keystroke "v" using command down"#])
        .output();

    match result {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("osascript paste failed: {}", stderr.trim());
            // Fallback: try CGEvent (works if Accessibility permission is granted)
            simulate_key_combo(K_VK_ANSI_V, K_CG_EVENT_FLAG_MASK_COMMAND);
        }
        Err(e) => {
            eprintln!("osascript not available: {}", e);
            simulate_key_combo(K_VK_ANSI_V, K_CG_EVENT_FLAG_MASK_COMMAND);
        }
    }

    // Text is on clipboard regardless — user can always Cmd+V manually
    Ok(())
}
