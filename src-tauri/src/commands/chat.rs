use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use super::generate::GenerateResult;

// --- Types ---

#[derive(Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessageEntry {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub format: String,
    pub created_at: String,
    pub updated_at: String,
    pub total_tokens: u64,
    pub messages: Vec<ChatMessageEntry>,
}

#[derive(Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub format: String,
    pub updated_at: String,
    pub message_count: usize,
    pub total_tokens: u64,
}

// --- Helpers ---

fn chats_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = std::path::PathBuf::from(home).join(".noren").join("chats");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Build a chat-specific system prompt from the voice profile.
fn build_chat_system_prompt(core_identity: &str, context_layer: Option<&str>) -> String {
    let mut prompt = String::from(
        "You are a helpful writing assistant. Be conversational and helpful.",
    );
    if !core_identity.is_empty() {
        prompt.push_str(
            " Write in the user's voice and style as described in their profile:\n\n",
        );
        prompt.push_str(core_identity);
    }
    if let Some(ctx) = context_layer {
        prompt.push_str("\n\nAdditional context for this format:\n");
        prompt.push_str(ctx);
    }
    prompt
}

// --- Chat send command ---

#[tauri::command]
pub async fn chat_send(
    state: State<'_, AppState>,
    messages: Vec<ChatMessage>,
    format: String,
    attachments: Option<Vec<String>>,
) -> Result<GenerateResult, String> {
    let config = state.config.lock().unwrap().clone();

    // Load local profile for voice-aware system prompt
    let (core_identity, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), std::collections::HashMap::new()));

    let context_layer = contexts.get(&format);
    let system_prompt = build_chat_system_prompt(&core_identity, context_layer.map(String::as_str));

    // Build full message array: system prompt + conversation history
    let mut llm_messages = vec![noren_engine::LlmMessage {
        role: noren_engine::Role::System,
        content: system_prompt,
    }];

    let last_user_idx = messages.iter().rposition(|m| m.role == "user");

    for (i, msg) in messages.iter().enumerate() {
        let role = match msg.role.as_str() {
            "assistant" => noren_engine::Role::Assistant,
            _ => noren_engine::Role::User,
        };

        // Prepend attachment contents to the last user message
        let content = if Some(i) == last_user_idx {
            if let Some(ref atts) = attachments {
                if !atts.is_empty() {
                    let mut parts = Vec::new();
                    for (j, att) in atts.iter().enumerate() {
                        parts.push(format!("[Attached file {}]\n{}", j + 1, att));
                    }
                    parts.push(msg.content.clone());
                    parts.join("\n\n")
                } else {
                    msg.content.clone()
                }
            } else {
                msg.content.clone()
            }
        } else {
            msg.content.clone()
        };

        llm_messages.push(noren_engine::LlmMessage {
            role,
            content,
        });
    }

    let thinking = if config.extended_thinking {
        Some(noren_engine::ThinkingConfig {
            budget_tokens: config.thinking_budget,
        })
    } else {
        None
    };

    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(if config.extended_thinking { config.thinking_budget + 4096 } else { 4096 }),
        thinking,
    };

    // Create LLM client (same dual-path as generate)
    let client: Box<dyn noren_engine::LlmClient> =
        if config.inference_mode == noren_engine::InferenceMode::NorenPro {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.usenoren.ai")
                .to_string();
            let auth_token = crate::keychain::get_api_key("noren-pro-token")
                .ok_or("Not logged in to Noren Pro. Go to Settings to sign in.")?;
            Box::new(noren_engine::NorenProxyClient::new(
                server_url,
                auth_token,
                format,
            ))
        } else {
            let api_key = if config.provider.requires_key {
                crate::keychain::get_api_key(&config.provider.keychain_id())
            } else {
                None
            };
            noren_engine::create_llm_client(&config, api_key).map_err(|e| e.to_string())?
        };

    let response = client
        .complete(&llm_messages, &options)
        .await
        .map_err(|e| e.to_string())?;

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
    })
}

fn validate_chat_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("Invalid chat ID".to_string());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Invalid chat ID".to_string());
    }
    Ok(())
}

// --- Chat history commands ---

#[tauri::command]
pub fn save_chat(conversation: Conversation) -> Result<(), String> {
    validate_chat_id(&conversation.id)?;
    let dir = chats_dir();
    let path = dir.join(format!("{}.json", conversation.id));
    let json = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to save chat: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn list_chats() -> Result<Vec<ConversationSummary>, String> {
    let dir = chats_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut summaries = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                    summaries.push(ConversationSummary {
                        id: conv.id,
                        title: conv.title,
                        format: conv.format,
                        updated_at: conv.updated_at,
                        message_count: conv.messages.len(),
                        total_tokens: conv.total_tokens,
                    });
                }
            }
        }
    }

    // Most recent first
    summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(summaries)
}

#[tauri::command]
pub fn load_chat(id: String) -> Result<Conversation, String> {
    validate_chat_id(&id)?;
    let path = chats_dir().join(format!("{}.json", id));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read chat: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse chat: {}", e))
}

#[tauri::command]
pub fn delete_chat(id: String) -> Result<(), String> {
    validate_chat_id(&id)?;
    let path = chats_dir().join(format!("{}.json", id));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete chat: {}", e))?;
    }
    Ok(())
}
