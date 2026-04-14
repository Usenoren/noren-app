use serde::{Deserialize, Serialize};
use tauri::{Emitter, State, Window};

use super::generate::GenerateResult;
use crate::AppState;

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

/// Build a chat system prompt. No voice enforcement, just helpful assistant
/// with optional context about the user's domain.
fn build_chat_system_prompt(context_layer: Option<&str>) -> String {
    let mut prompt =
        String::from("You are a helpful assistant. Be conversational, clear, and concise.");
    if let Some(ctx) = context_layer {
        prompt.push_str("\n\nContext about the user's work:\n");
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
    chat_id: Option<String>,
    chat_title: Option<String>,
) -> Result<GenerateResult, String> {
    let config = state.config.lock().unwrap().clone();

    // Load context layer only (no voice profile for chat)
    let (_, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), std::collections::HashMap::new()));

    let context_layer = contexts.get(&format);
    let system_prompt = build_chat_system_prompt(context_layer.map(String::as_str));

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

        llm_messages.push(noren_engine::LlmMessage { role, content });
    }

    let thinking = if config.extended_thinking {
        Some(noren_engine::ThinkingConfig {
            budget_tokens: config.thinking_budget,
        })
    } else {
        None
    };

    let use_cache = config.provider.provider_type == noren_engine::ProviderType::Anthropic;
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(if config.extended_thinking {
            config.thinking_budget + 4096
        } else {
            4096
        }),
        thinking,
        cache: if use_cache { Some(true) } else { None },
        chat_id,
        chat_title,
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
                .ok_or("Not signed in to Noren. Go to Settings to sign in.")?;
            let refresh_token = crate::keychain::get_api_key("noren-pro-refresh");
            let mut proxy = noren_engine::NorenProxyClient::new(server_url, auth_token, format);
            if let Some(rt) = refresh_token {
                proxy = proxy.with_token_refresh(rt, |new_access, new_refresh| {
                    let _ = crate::keychain::store_api_key("noren-pro-token", &new_access);
                    let _ = crate::keychain::store_api_key("noren-pro-refresh", &new_refresh);
                });
            }
            Box::new(proxy)
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
        .map_err(|e| crate::auth_client::normalize_auth_error(e.to_string()))?;

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        voice_check: None,
        routed_model: None,
        route_reason: None,
    })
}

#[derive(Clone, Serialize)]
struct ChatChunk {
    text: String,
}

#[derive(Clone, Serialize)]
struct ChatDone {
    content: String,
    input_tokens: u64,
    output_tokens: u64,
}

#[tauri::command]
pub async fn chat_send_stream(
    window: Window,
    state: State<'_, AppState>,
    messages: Vec<ChatMessage>,
    format: String,
    attachments: Option<Vec<String>>,
    chat_id: Option<String>,
    chat_title: Option<String>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    let (_, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), std::collections::HashMap::new()));
    let context_layer = contexts.get(&format);
    let system_prompt = build_chat_system_prompt(context_layer.map(String::as_str));

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
        let content = if Some(i) == last_user_idx {
            if let Some(ref atts) = attachments {
                if !atts.is_empty() {
                    let mut parts: Vec<String> = atts
                        .iter()
                        .enumerate()
                        .map(|(j, att)| format!("[Attached file {}]\n{}", j + 1, att))
                        .collect();
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
        llm_messages.push(noren_engine::LlmMessage { role, content });
    }

    let thinking = if config.extended_thinking {
        Some(noren_engine::ThinkingConfig {
            budget_tokens: config.thinking_budget,
        })
    } else {
        None
    };

    let use_cache = config.provider.provider_type == noren_engine::ProviderType::Anthropic;
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(if config.extended_thinking {
            config.thinking_budget + 4096
        } else {
            4096
        }),
        thinking,
        cache: if use_cache { Some(true) } else { None },
        chat_id,
        chat_title,
    };

    let client: Box<dyn noren_engine::LlmClient> =
        if config.inference_mode == noren_engine::InferenceMode::NorenPro {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.usenoren.ai")
                .to_string();
            let auth_token = crate::keychain::get_api_key("noren-pro-token")
                .ok_or("Not signed in to Noren.")?;
            let refresh_token = crate::keychain::get_api_key("noren-pro-refresh");
            let mut proxy = noren_engine::NorenProxyClient::new(server_url, auth_token, format);
            if let Some(rt) = refresh_token {
                proxy = proxy.with_token_refresh(rt, |a, r| {
                    let _ = crate::keychain::store_api_key("noren-pro-token", &a);
                    let _ = crate::keychain::store_api_key("noren-pro-refresh", &r);
                });
            }
            Box::new(proxy)
        } else {
            let api_key = if config.provider.requires_key {
                crate::keychain::get_api_key(&config.provider.keychain_id())
            } else {
                None
            };
            noren_engine::create_llm_client(&config, api_key).map_err(|e| e.to_string())?
        };

    let w = window.clone();
    let on_chunk: noren_engine::StreamCallback = Box::new(move |text: &str| {
        let _ = w.emit(
            "chat:chunk",
            ChatChunk {
                text: text.to_string(),
            },
        );
    });

    let response = client
        .stream_complete(&llm_messages, &options, on_chunk)
        .await
        .map_err(|e| crate::auth_client::normalize_auth_error(e.to_string()))?;

    let _ = window.emit(
        "chat:done",
        ChatDone {
            content: response.content,
            input_tokens: response.input_tokens,
            output_tokens: response.output_tokens,
        },
    );
    Ok(())
}

fn validate_chat_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("Invalid chat ID".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
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
    std::fs::write(&path, json).map_err(|e| format!("Failed to save chat: {}", e))?;
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
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read chat: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse chat: {}", e))
}

#[tauri::command]
pub fn delete_chat(id: String) -> Result<(), String> {
    validate_chat_id(&id)?;
    let path = chats_dir().join(format!("{}.json", id));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete chat: {}", e))?;
    }
    Ok(())
}

/// Pull chats from server and merge into local storage (Pro users only).
/// Returns the number of chats synced.
#[tauri::command]
pub async fn sync_chats_from_server(state: State<'_, AppState>) -> Result<u32, String> {
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode != noren_engine::InferenceMode::NorenPro {
        return Ok(0);
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .trim_end_matches('/')
        .to_string();

    if crate::keychain::get_api_key("noren-pro-token").is_none() {
        return Ok(0);
    }

    // 1. Get manifest
    let manifest_url = format!("{}/v1/sync/chats/manifest", server_url);
    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client.get(&manifest_url).bearer_auth(token)
    })
    .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(0),
    };

    let manifest: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let chats = manifest["chats"].as_array().ok_or("Invalid manifest")?;

    let dir = chats_dir();
    let mut synced: u32 = 0;

    for entry in chats {
        let chat_id = entry["chat_id"].as_str().unwrap_or_default();
        if chat_id.is_empty() || validate_chat_id(chat_id).is_err() {
            continue;
        }

        let is_deleted = entry["is_deleted"].as_bool().unwrap_or(false);
        let remote_updated = entry["updated_at"].as_str().unwrap_or_default();
        let local_path = dir.join(format!("{}.json", chat_id));

        if is_deleted {
            let _ = std::fs::remove_file(&local_path);
            continue;
        }

        // Skip if local is up to date
        if local_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&local_path) {
                if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                    if conv.updated_at >= remote_updated.to_string() {
                        continue;
                    }
                }
            }
        }

        // Download from server
        let dl_url = format!("{}/v1/sync/chats/{}", server_url, chat_id);
        let dl_resp = crate::auth_client::authed_request(&server_url, |client, token| {
            client.get(&dl_url).bearer_auth(token)
        })
        .await;

        if let Ok(resp) = dl_resp {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    let title = data["title"].as_str().unwrap_or("Untitled").to_string();
                    let updated_at = data["updated_at"].as_str().unwrap_or_default().to_string();
                    let messages_val = &data["messages"];

                    let mut msgs = Vec::new();
                    if let Some(arr) = messages_val.as_array() {
                        for m in arr {
                            msgs.push(ChatMessageEntry {
                                role: m["role"].as_str().unwrap_or("user").to_string(),
                                content: m["content"].as_str().unwrap_or_default().to_string(),
                            });
                        }
                    }

                    let conv = Conversation {
                        id: chat_id.to_string(),
                        title,
                        format: "general".to_string(),
                        created_at: updated_at.clone(),
                        updated_at,
                        total_tokens: 0,
                        messages: msgs,
                    };

                    if let Ok(json) = serde_json::to_string_pretty(&conv) {
                        let _ = std::fs::write(&local_path, json);
                        synced += 1;
                    }
                }
            }
        }
    }

    Ok(synced)
}

/// Delete a chat on the server (Pro users only). Fire-and-forget.
#[tauri::command]
pub async fn sync_delete_chat(state: State<'_, AppState>, id: String) -> Result<(), String> {
    validate_chat_id(&id)?;
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode != noren_engine::InferenceMode::NorenPro {
        return Ok(()); // BYOK — nothing to sync
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .trim_end_matches('/')
        .to_string();

    if crate::keychain::get_api_key("noren-pro-token").is_none() {
        return Ok(()); // Not logged in
    }

    let url = format!("{}/v1/sync/chats/{}", server_url, id);
    let _ = crate::auth_client::authed_request(&server_url, |client, token| {
        client.delete(&url).bearer_auth(token)
    })
    .await;

    Ok(())
}
