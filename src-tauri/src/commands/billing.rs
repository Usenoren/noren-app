use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::AppState;

// --- Structs ---

#[derive(Serialize, Deserialize)]
pub struct SubscriptionStatus {
    pub tier: String,
    pub active: bool,
    pub email_verified: bool,
    pub is_founding_member: bool,
    pub can_extract: bool,
    pub can_generate_bundled: bool,
    pub can_living_profile: bool,
    pub can_sync: bool,
    pub can_export: bool,
    pub tokens_limit: u64,
    pub generations_limit: u64,
    pub is_trial: bool,
    pub trial_expires_at: Option<String>,
    pub current_period_end: Option<String>,
    pub cancel_at_period_end: bool,
    pub one_time_purchases: Vec<String>,
    pub extraction_credits_remaining: Option<u64>,
    pub export_unlock_remaining_cents: Option<u64>,
    pub export_unlock_progress: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BillingPublicConfig {
    pub pro_monthly_amount_label: String,
    pub pro_monthly_interval_label: String,
    pub pro_monthly_full_label: String,
    pub pro_pricing_note: String,
    pub pro_founding_monthly_amount_label: String,
    pub pro_founding_monthly_full_label: String,
    pub pro_founding_pricing_note: String,
    pub extraction_amount_label: String,
    pub extraction_cta_label: String,
    pub extraction_founding_amount_label: String,
    pub extraction_founding_cta_label: String,
    pub default_trial_days: u32,
}

#[derive(Serialize)]
pub struct CheckoutResult {
    pub checkout_url: String,
    pub session_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExtractionReceipt {
    pub extraction_granted: bool,
    pub session_id: String,
    pub granted_at: String,
    #[serde(default)]
    pub used: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PendingCheckout {
    pub session_id: String,
    pub email: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct GuestCheckoutStatus {
    pub paid: bool,
    pub tier: String,
}

#[derive(Serialize)]
pub struct RestoreResult {
    pub found: bool,
    pub session_id: Option<String>,
}

// --- Path helpers ---

fn noren_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".noren")
}

fn receipt_path() -> PathBuf {
    noren_dir().join("extraction_receipt.json")
}

fn pending_path() -> PathBuf {
    noren_dir().join("extraction_pending.json")
}

pub(crate) fn server_url_from_config(state: &State<'_, AppState>) -> String {
    let config = state.config.lock().unwrap();
    config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .to_string()
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let mut y = 1970i64;
    let mut remaining = days_since_epoch as i64;

    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut mo = 0;
    for (i, &d) in month_days.iter().enumerate() {
        if remaining < d as i64 {
            mo = i + 1;
            break;
        }
        remaining -= d as i64;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, mo, remaining + 1, h, m, s
    )
}

// --- Existing auth-required commands ---

#[tauri::command]
pub async fn get_billing_public_config(
    state: State<'_, AppState>,
) -> Result<BillingPublicConfig, String> {
    let server_url = server_url_from_config(&state);
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/billing/public-config", server_url))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get billing config: {}", body));
    }

    resp.json::<BillingPublicConfig>()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_subscription_status(
    state: State<'_, AppState>,
) -> Result<SubscriptionStatus, String> {
    let server_url = server_url_from_config(&state);

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .get(format!("{}/v1/billing/status", server_url))
            .bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to get subscription: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    let ents = &data["entitlements"];

    Ok(SubscriptionStatus {
        tier: data["tier"].as_str().unwrap_or("free").to_string(),
        active: data["active"].as_bool().unwrap_or(false),
        email_verified: data["email_verified"].as_bool().unwrap_or(true),
        is_founding_member: data["is_founding_member"].as_bool().unwrap_or(false),
        can_extract: ents["can_extract"].as_bool().unwrap_or(false),
        can_generate_bundled: ents["can_generate_bundled"].as_bool().unwrap_or(false),
        can_living_profile: ents["can_living_profile"].as_bool().unwrap_or(false),
        can_sync: ents["can_sync"].as_bool().unwrap_or(false),
        can_export: ents["can_export"].as_bool().unwrap_or(false),
        tokens_limit: ents["tokens_limit"].as_u64().unwrap_or(0),
        generations_limit: ents["generations_limit"].as_u64().unwrap_or(0),
        is_trial: ents["is_trial"].as_bool().unwrap_or(false),
        trial_expires_at: ents["trial_expires_at"].as_str().map(|s| s.to_string()),
        current_period_end: data["current_period_end"].as_str().map(|s| s.to_string()),
        cancel_at_period_end: data["cancel_at_period_end"].as_bool().unwrap_or(false),
        one_time_purchases: data["one_time_purchases"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        extraction_credits_remaining: data["extraction_credits_remaining"].as_u64(),
        export_unlock_remaining_cents: data["export_unlock_remaining_cents"].as_u64(),
        export_unlock_progress: data["export_unlock_progress"].as_u64(),
    })
}

#[derive(Serialize)]
pub struct CouponRedeemResult {
    pub message: String,
    pub tier: String,
    pub trial_days: u32,
    pub trial_expires_at: String,
}

#[tauri::command]
pub async fn create_checkout(
    state: State<'_, AppState>,
    tier: String,
    coupon_code: Option<String>,
) -> Result<CheckoutResult, String> {
    let server_url = server_url_from_config(&state);

    let tier_clone = tier.clone();
    let code_clone = coupon_code.clone();
    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        let mut body = serde_json::json!({ "target": tier_clone });
        if let Some(ref code) = code_clone {
            let trimmed = code.trim();
            if !trimmed.is_empty() {
                body["coupon_code"] = serde_json::Value::String(trimmed.to_string());
            }
        }
        client
            .post(format!("{}/v1/billing/checkout", server_url))
            .bearer_auth(token)
            .json(&body)
    })
    .await?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Checkout failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(CheckoutResult {
        checkout_url: data["checkout_url"]
            .as_str()
            .ok_or("No checkout URL in response")?
            .to_string(),
        session_id: data["session_id"]
            .as_str()
            .ok_or("No session ID in response")?
            .to_string(),
    })
}

#[tauri::command]
pub async fn create_export_unlock_checkout(
    state: State<'_, AppState>,
) -> Result<CheckoutResult, String> {
    let server_url = server_url_from_config(&state);

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/billing/checkout/export-unlock", server_url))
            .bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Export unlock checkout failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(CheckoutResult {
        checkout_url: data["checkout_url"]
            .as_str()
            .ok_or("No checkout URL in response")?
            .to_string(),
        session_id: data["session_id"]
            .as_str()
            .ok_or("No session ID in response")?
            .to_string(),
    })
}

#[tauri::command]
pub async fn open_billing_portal(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let server_url = server_url_from_config(&state);

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/billing/portal", server_url))
            .bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        return Err(format!("Portal failed: {}", body));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    data["portal_url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No portal URL in response".to_string())
}

#[tauri::command]
pub async fn redeem_coupon(
    state: State<'_, AppState>,
    code: String,
) -> Result<CouponRedeemResult, String> {
    let server_url = server_url_from_config(&state);
    let code_clone = code.clone();

    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/billing/redeem-coupon", server_url))
            .bearer_auth(token)
            .json(&serde_json::json!({ "code": code_clone }))
    })
    .await?;

    let status = resp.status().as_u16();

    if !resp.status().is_success() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let detail = body["detail"]
            .as_str()
            .unwrap_or("Coupon redemption failed");
        return Err(format!("{}:{}", status, detail));
    }

    let data: serde_json::Value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;

    Ok(CouponRedeemResult {
        message: data["message"]
            .as_str()
            .unwrap_or("Trial activated")
            .to_string(),
        tier: data["tier"]
            .as_str()
            .unwrap_or("pro")
            .to_string(),
        trial_days: data["trial_days"]
            .as_u64()
            .unwrap_or(0) as u32,
        trial_expires_at: data["trial_expires_at"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    })
}

// --- Guest checkout commands (no auth required) ---

#[tauri::command]
pub async fn create_guest_checkout(
    state: State<'_, AppState>,
    email: String,
    tier: String,
) -> Result<CheckoutResult, String> {
    if tier != "extraction" {
        return Err("Guest checkout is only available for extraction.".to_string());
    }

    let server_url = server_url_from_config(&state);
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/billing/checkout/guest", server_url))
        .json(&serde_json::json!({ "email": email, "tier": tier }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Checkout failed: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    Ok(CheckoutResult {
        checkout_url: data["checkout_url"]
            .as_str()
            .ok_or("No checkout URL in response")?
            .to_string(),
        session_id: data["session_id"]
            .as_str()
            .ok_or("No session ID in response")?
            .to_string(),
    })
}

#[tauri::command]
pub async fn poll_guest_checkout(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<GuestCheckoutStatus, String> {
    let server_url = server_url_from_config(&state);
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/billing/checkout/status/{}",
            server_url, session_id
        ))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if resp.status().as_u16() == 404 {
        return Err("Session not found.".to_string());
    }

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Status check failed: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    Ok(GuestCheckoutStatus {
        paid: data["paid"].as_bool().unwrap_or(false),
        tier: data["tier"].as_str().unwrap_or("extraction").to_string(),
    })
}

#[tauri::command]
pub async fn restore_guest_purchase(
    state: State<'_, AppState>,
    email: String,
) -> Result<RestoreResult, String> {
    let server_url = server_url_from_config(&state);
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/billing/checkout/restore", server_url))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Restore failed: {}", body));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    Ok(RestoreResult {
        found: data["found"].as_bool().unwrap_or(false),
        session_id: data["session_id"].as_str().map(|s| s.to_string()),
    })
}

// --- Local receipt commands ---

#[tauri::command]
pub fn store_extraction_receipt(session_id: String) -> Result<(), String> {
    let receipt = ExtractionReceipt {
        extraction_granted: true,
        session_id,
        granted_at: now_iso(),
        used: false,
    };
    let dir = noren_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?;
    std::fs::write(receipt_path(), json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn has_extraction_receipt() -> bool {
    match std::fs::read_to_string(receipt_path()) {
        Ok(data) => match serde_json::from_str::<ExtractionReceipt>(&data) {
            Ok(r) => r.extraction_granted && !r.used,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[tauri::command]
pub fn has_used_extraction() -> bool {
    match std::fs::read_to_string(receipt_path()) {
        Ok(data) => match serde_json::from_str::<ExtractionReceipt>(&data) {
            Ok(r) => r.extraction_granted && r.used,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[tauri::command]
pub fn mark_extraction_used() -> Result<(), String> {
    let data = std::fs::read_to_string(receipt_path())
        .map_err(|e| format!("No receipt found: {}", e))?;
    let mut receipt: ExtractionReceipt =
        serde_json::from_str(&data).map_err(|e| format!("Invalid receipt: {}", e))?;
    receipt.used = true;
    let json = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?;
    std::fs::write(receipt_path(), json).map_err(|e| e.to_string())?;
    Ok(())
}

// --- Pending checkout commands ---

#[tauri::command]
pub fn store_pending_checkout(session_id: String, email: String) -> Result<(), String> {
    let pending = PendingCheckout {
        session_id,
        email,
        created_at: now_iso(),
    };
    let dir = noren_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&pending).map_err(|e| e.to_string())?;
    std::fs::write(pending_path(), json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_pending_checkout() -> Option<PendingCheckout> {
    std::fs::read_to_string(pending_path())
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
}

#[tauri::command]
pub fn clear_pending_checkout() -> Result<(), String> {
    let path = pending_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
