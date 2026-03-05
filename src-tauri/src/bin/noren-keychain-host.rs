//! Chrome Native Messaging host for macOS Keychain access.
//!
//! Speaks the Chrome native messaging protocol:
//! - Reads 4-byte little-endian length prefix + JSON from stdin
//! - Writes 4-byte little-endian length prefix + JSON to stdout

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::io::{self, Read, Write};

const SERVICE: &str = "ink.noren.app";

fn read_message() -> io::Result<serde_json::Value> {
    let mut len_buf = [0u8; 4];
    io::stdin().read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut buf = vec![0u8; len];
    io::stdin().read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_message(msg: &serde_json::Value) -> io::Result<()> {
    let data = serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let len = (data.len() as u32).to_le_bytes();
    let mut out = io::stdout().lock();
    out.write_all(&len)?;
    out.write_all(&data)?;
    out.flush()
}

fn handle(msg: serde_json::Value) -> serde_json::Value {
    let action = msg.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let provider = msg.get("provider").and_then(|v| v.as_str()).unwrap_or("");

    if provider.is_empty() && action != "ping" {
        return serde_json::json!({ "ok": false, "error": "missing provider" });
    }

    let account = format!("api-key-{}", provider);

    match action {
        "ping" => serde_json::json!({ "ok": true }),

        "get" => match get_generic_password(SERVICE, &account) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(key) => serde_json::json!({ "ok": true, "key": key }),
                Err(_) => serde_json::json!({ "ok": false, "error": "invalid utf8" }),
            },
            Err(_) => serde_json::json!({ "ok": true, "key": null }),
        },

        "store" => {
            let key = msg.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if key.is_empty() {
                return serde_json::json!({ "ok": false, "error": "missing key" });
            }
            match set_generic_password(SERVICE, &account, key.as_bytes()) {
                Ok(_) => serde_json::json!({ "ok": true }),
                Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
            }
        }

        "delete" => match delete_generic_password(SERVICE, &account) {
            Ok(_) => serde_json::json!({ "ok": true }),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
        },

        "has" => {
            let has = get_generic_password(SERVICE, &account).is_ok();
            serde_json::json!({ "ok": true, "has_key": has })
        }

        _ => serde_json::json!({ "ok": false, "error": "unknown action" }),
    }
}

fn main() {
    // Chrome sends one message per launch for sendNativeMessage
    match read_message() {
        Ok(msg) => {
            let response = handle(msg);
            let _ = write_message(&response);
        }
        Err(e) => {
            let _ = write_message(&serde_json::json!({
                "ok": false,
                "error": e.to_string()
            }));
        }
    }
}
