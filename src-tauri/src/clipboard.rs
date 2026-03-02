//! Clipboard management and text injection.

use std::ffi::c_void;
use std::io::Write;
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
        if !key_down.is_null() {
            CGEventSetFlags(key_down, flags);
            CGEventPost(K_CG_HID_EVENT_TAP, key_down);
        }

        // Small delay between key-down and key-up for reliability
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Key up
        let key_up = CGEventCreateKeyboardEvent(source, key, false);
        if !key_up.is_null() {
            CGEventSetFlags(key_up, flags);
            CGEventPost(K_CG_HID_EVENT_TAP, key_up);
        }

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

/// Wait until the given PID becomes the frontmost application.
/// Returns true if the app was activated within the timeout.
fn wait_for_app_focus(pid: i32, timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    loop {
        if let Some(front) = accessibility::get_frontmost_pid() {
            if front == pid {
                return true;
            }
        }
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Write text to the macOS clipboard. Tries Tauri plugin first, then pbcopy.
fn write_clipboard(app: &AppHandle, text: &str) -> Result<(), String> {
    if app.clipboard().write_text(text).is_ok() {
        return Ok(());
    }
    eprintln!("[inject] Tauri clipboard failed, trying pbcopy");
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("pbcopy failed to start: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("pbcopy write failed: {}", e))?;
    }
    child.wait().map_err(|e| format!("pbcopy failed: {}", e))?;
    Ok(())
}

/// Inject text into the source application.
/// Must be called from a background thread (NOT the main thread) so that
/// window.hide() can complete while we wait.
///
/// 1. Writes text to clipboard
/// 2. Waits for the Noren window to fully hide
/// 3. Activates the source app by PID with polling confirmation
/// 4. Simulates Cmd+V via CGEvent
pub fn inject_text(
    app: &AppHandle,
    text: &str,
    source_pid: Option<i32>,
    _source_app_name: Option<String>,
) -> Result<(), String> {
    // Step 1: Write text to clipboard
    write_clipboard(app, text)?;

    // Step 2: Wait for the Noren window to fully hide.
    // hide() was called on the main thread — give the event loop time to process it.
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Step 3: Activate source app and poll until it's frontmost
    if let Some(pid) = source_pid {
        accessibility::activate_app(pid);
        if !wait_for_app_focus(pid, 3000) {
            eprintln!("[inject] timeout waiting for source app to activate");
        }
    } else {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Step 4: Extra stabilization — let the app settle after activation
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Step 5: Paste via CGEvent Cmd+V
    simulate_key_combo(K_VK_ANSI_V, K_CG_EVENT_FLAG_MASK_COMMAND);

    Ok(())
}
