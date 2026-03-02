//! macOS Accessibility API bindings for reading selected text from any application,
//! and NSWorkspace/NSRunningApplication bindings for app activation.

use std::ffi::c_void;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type AXUIElementRef = *const c_void;
type AXError = i32;

const K_AX_ERROR_SUCCESS: AXError = 0;
const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

// Objective-C runtime for NSWorkspace / NSRunningApplication
#[link(name = "objc", kind = "dylib")]
extern "C" {
    fn objc_getClass(name: *const i8) -> *const c_void;
    fn sel_registerName(name: *const i8) -> *const c_void;
    fn objc_msgSend();
}

// Ensure AppKit is linked (for NSWorkspace, NSRunningApplication)
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: CFTypeRef,
        c_str: *const i8,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetLength(string: CFStringRef) -> isize;
    fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut u8,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFDictionaryCreate(
        allocator: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);

    static kCFBooleanTrue: CFTypeRef;
    static kCFBooleanFalse: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

/// Create a CFString from a Rust string. Caller must CFRelease the result.
unsafe fn cfstring_create(s: &str) -> CFStringRef {
    let c_str = std::ffi::CString::new(s).unwrap();
    CFStringCreateWithCString(std::ptr::null(), c_str.as_ptr(), K_CF_STRING_ENCODING_UTF8)
}

/// Convert a CFStringRef to a Rust String.
unsafe fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    let len = CFStringGetLength(cf);
    if len <= 0 {
        return None;
    }
    // UTF-8 can be up to 4 bytes per character
    let buf_size = len * 4 + 1;
    let mut buf = vec![0u8; buf_size as usize];
    if CFStringGetCString(cf, buf.as_mut_ptr(), buf_size, K_CF_STRING_ENCODING_UTF8) {
        let nul_pos = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..nul_pos]).into_owned())
    } else {
        None
    }
}

/// Check if the app has accessibility permissions.
/// If `prompt` is true, macOS will show the system permission dialog.
pub fn check_accessibility_trusted(prompt: bool) -> bool {
    unsafe {
        let value = if prompt {
            kCFBooleanTrue
        } else {
            kCFBooleanFalse
        };
        let keys = [kAXTrustedCheckOptionPrompt as CFTypeRef];
        let values = [value];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const c_void,
        );
        let result = AXIsProcessTrustedWithOptions(options);
        if !options.is_null() {
            CFRelease(options);
        }
        result
    }
}

/// Try to get the selected text from the frontmost application using the Accessibility API.
/// Returns None if no text is selected or accessibility permissions are not granted.
pub fn get_selected_text_ax() -> Option<String> {
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }

        // Get focused UI element
        let attr = cfstring_create("AXFocusedUIElement");
        let mut focused_element: CFTypeRef = std::ptr::null();
        let err =
            AXUIElementCopyAttributeValue(system_wide, attr, &mut focused_element);
        CFRelease(attr);
        CFRelease(system_wide);

        if err != K_AX_ERROR_SUCCESS || focused_element.is_null() {
            return None;
        }

        // Get selected text from focused element
        let attr = cfstring_create("AXSelectedText");
        let mut selected_text: CFTypeRef = std::ptr::null();
        let err =
            AXUIElementCopyAttributeValue(focused_element, attr, &mut selected_text);
        CFRelease(attr);
        CFRelease(focused_element);

        if err != K_AX_ERROR_SUCCESS || selected_text.is_null() {
            return None;
        }

        let result = cfstring_to_string(selected_text);
        CFRelease(selected_text);
        result
    }
}

/// Get the PID of the currently frontmost application via NSWorkspace.
pub fn get_frontmost_pid() -> Option<i32> {
    unsafe {
        let workspace_class = objc_getClass(b"NSWorkspace\0".as_ptr() as *const i8);
        if workspace_class.is_null() {
            return None;
        }

        let shared_sel = sel_registerName(b"sharedWorkspace\0".as_ptr() as *const i8);
        let front_sel = sel_registerName(b"frontmostApplication\0".as_ptr() as *const i8);
        let pid_sel = sel_registerName(b"processIdentifier\0".as_ptr() as *const i8);

        // [NSWorkspace sharedWorkspace]
        let send_ptr: unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let workspace = send_ptr(workspace_class, shared_sel);
        if workspace.is_null() {
            return None;
        }

        // [workspace frontmostApplication]
        let front_app = send_ptr(workspace, front_sel);
        if front_app.is_null() {
            return None;
        }

        // [frontApp processIdentifier] -> pid_t (i32)
        let send_i32: unsafe extern "C" fn(*const c_void, *const c_void) -> i32 =
            std::mem::transmute(objc_msgSend as *const ());
        let pid = send_i32(front_app, pid_sel);

        if pid > 0 { Some(pid) } else { None }
    }
}

/// Get the localized name of the frontmost application.
pub fn get_frontmost_app_name() -> Option<String> {
    unsafe {
        let workspace_class = objc_getClass(b"NSWorkspace\0".as_ptr() as *const i8);
        if workspace_class.is_null() {
            return None;
        }

        let shared_sel = sel_registerName(b"sharedWorkspace\0".as_ptr() as *const i8);
        let front_sel = sel_registerName(b"frontmostApplication\0".as_ptr() as *const i8);
        let name_sel = sel_registerName(b"localizedName\0".as_ptr() as *const i8);

        let send_ptr: unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const ());

        let workspace = send_ptr(workspace_class, shared_sel);
        if workspace.is_null() {
            return None;
        }

        let front_app = send_ptr(workspace, front_sel);
        if front_app.is_null() {
            return None;
        }

        // localizedName returns an NSString (which is toll-free bridged with CFString)
        let name_ref = send_ptr(front_app, name_sel);
        if name_ref.is_null() {
            return None;
        }

        cfstring_to_string(name_ref as CFStringRef)
    }
}

/// Map an application name to its likely writing format context.
pub fn detect_format(app_name: &str) -> Option<&'static str> {
    let lower = app_name.to_lowercase();
    match lower.as_str() {
        // Social
        "twitter" | "x" | "tweetbot" | "twitterrific" => Some("twitter"),
        "linkedin" => Some("linkedin"),
        // Messaging
        "slack" | "messages" | "imessage" | "discord" | "telegram" | "whatsapp"
        | "microsoft teams" | "teams" => Some("slack"),
        // Email
        "mail" | "gmail" | "outlook" | "spark" | "airmail" | "mimestream"
        | "microsoft outlook" | "thunderbird" => Some("email"),
        // Long-form
        "notion" | "obsidian" | "bear" | "ulysses" | "ia writer" | "typora"
        | "google docs" | "pages" | "word" | "microsoft word" | "scrivener"
        | "craft" | "logseq" | "roam research" | "substack" => Some("longform"),
        _ => {
            // Browser heuristics — could be anything, don't auto-detect
            if lower.contains("safari") || lower.contains("chrome")
                || lower.contains("firefox") || lower.contains("arc")
                || lower.contains("brave") || lower.contains("edge")
            {
                None
            } else {
                None
            }
        }
    }
}

/// Execute an AppleScript in-process using NSAppleScript.
/// This runs with the Noren app's own accessibility permissions, unlike
/// spawning /usr/bin/osascript which is a separate process without permissions.
#[allow(dead_code)]
pub fn run_applescript(script: &str) -> bool {
    unsafe {
        let nsapplescript_class =
            objc_getClass(b"NSAppleScript\0".as_ptr() as *const i8);
        let nsstring_class =
            objc_getClass(b"NSString\0".as_ptr() as *const i8);

        if nsapplescript_class.is_null() || nsstring_class.is_null() {
            return false;
        }

        let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const i8);
        let init_utf8_sel =
            sel_registerName(b"initWithUTF8String:\0".as_ptr() as *const i8);
        let init_source_sel =
            sel_registerName(b"initWithSource:\0".as_ptr() as *const i8);
        let execute_sel =
            sel_registerName(b"executeAndReturnError:\0".as_ptr() as *const i8);
        let release_sel = sel_registerName(b"release\0".as_ptr() as *const i8);

        let send_ptr: unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_with_ptr: unsafe extern "C" fn(
            *const c_void,
            *const c_void,
            *const c_void,
        ) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void: unsafe extern "C" fn(*const c_void, *const c_void) =
            std::mem::transmute(objc_msgSend as *const ());

        // Create NSString from script
        let c_script = match std::ffi::CString::new(script) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let ns_string_alloc = send_ptr(nsstring_class, alloc_sel);
        if ns_string_alloc.is_null() {
            return false;
        }
        let ns_string = send_with_ptr(
            ns_string_alloc,
            init_utf8_sel,
            c_script.as_ptr() as *const c_void,
        );
        if ns_string.is_null() {
            return false;
        }

        // Create NSAppleScript from source
        let as_alloc = send_ptr(nsapplescript_class, alloc_sel);
        if as_alloc.is_null() {
            send_void(ns_string, release_sel);
            return false;
        }
        let apple_script = send_with_ptr(as_alloc, init_source_sel, ns_string);
        if apple_script.is_null() {
            send_void(ns_string, release_sel);
            return false;
        }

        // Execute: [appleScript executeAndReturnError:nil]
        let _result = send_with_ptr(apple_script, execute_sel, std::ptr::null());

        // Cleanup
        send_void(apple_script, release_sel);
        send_void(ns_string, release_sel);

        true
    }
}

/// Activate (bring to front) the application with the given PID.
pub fn activate_app(pid: i32) -> bool {
    unsafe {
        let ra_class = objc_getClass(b"NSRunningApplication\0".as_ptr() as *const i8);
        if ra_class.is_null() {
            return false;
        }

        let with_pid_sel = sel_registerName(
            b"runningApplicationWithProcessIdentifier:\0".as_ptr() as *const i8,
        );
        let activate_sel =
            sel_registerName(b"activateWithOptions:\0".as_ptr() as *const i8);

        // [NSRunningApplication runningApplicationWithProcessIdentifier:pid]
        let send_with_i32: unsafe extern "C" fn(*const c_void, *const c_void, i32) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let app = send_with_i32(ra_class, with_pid_sel, pid);
        if app.is_null() {
            return false;
        }

        // [app activateWithOptions:NSApplicationActivateIgnoringOtherApps]
        // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
        let send_activate: unsafe extern "C" fn(*const c_void, *const c_void, u64) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        send_activate(app, activate_sel, 2)
    }
}
