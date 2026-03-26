use tauri::State;

use crate::AppState;
use crate::commands::billing::server_url_from_config;

#[tauri::command]
pub async fn send_support_message(
    message: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let server_url = server_url_from_config(&state);
    let msg = message.clone();
    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(format!("{}/v1/support/message", server_url))
            .bearer_auth(token)
            .json(&serde_json::json!({ "message": msg }))
    })
    .await?;

    if !resp.status().is_success() {
        return Err("Failed to send message. Try again.".to_string());
    }
    Ok(())
}
