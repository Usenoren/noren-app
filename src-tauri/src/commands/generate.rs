use serde::Serialize;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
pub struct GenerateResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[tauri::command]
pub async fn generate(
    state: State<'_, AppState>,
    prompt: String,
    format: String,
    level: String,
    context: Option<String>,
    attachments: Option<Vec<String>>,
) -> Result<GenerateResult, String> {
    // Extract from state synchronously (no holding lock across await)
    let config = state.config.lock().unwrap().clone();
    let encryption_key = state.encryption_key;

    // Load profile
    let (core_identity, contexts) =
        noren_engine::load_profile(&config.profile_dir).map_err(|e| e.to_string())?;

    // Get context layer for this format
    let context_layer = contexts.get(&format);

    // Get enforcement prompt (cache → server → error)
    let cache_dir = noren_engine::prompt_cache::default_cache_dir();
    let enforcement_prompt = noren_engine::prompt_cache::get_enforcement_prompt(
        &cache_dir,
        &encryption_key,
        config.server_url.as_deref(),
        None, // auth token — Keychain integration in M6
    )
    .await
    .map_err(|e| e.to_string())?;

    // Parse enforcement level
    let enforcement_level = match level.as_str() {
        "strict" => noren_engine::EnforcementLevel::Strict,
        "light" => noren_engine::EnforcementLevel::Light,
        _ => noren_engine::EnforcementLevel::Guided,
    };

    // Compose system prompt
    let system_prompt = noren_engine::compose_system_prompt(
        &enforcement_prompt,
        &core_identity,
        context_layer.map(String::as_str),
        &format,
        &enforcement_level,
        &prompt,
    )
    .map_err(|e| e.to_string())?;

    // Build user message (include selected text context + attachments if present)
    let mut user_content = match context.filter(|s| !s.is_empty()) {
        Some(ctx) => format!("Context (selected text):\n{}\n\nRequest: {}", ctx, prompt),
        None => prompt,
    };

    // Append file attachments
    if let Some(ref attached) = attachments {
        for (i, content) in attached.iter().enumerate() {
            user_content.push_str(&format!(
                "\n\n--- Attached document {} ---\n{}",
                i + 1,
                content
            ));
        }
    }

    // Create LLM client — BYOK (direct) or Noren Pro (server proxy)
    let client: Box<dyn noren_engine::LlmClient> =
        if config.inference_mode == noren_engine::InferenceMode::NorenPro {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.noren.ink")
                .to_string();
            let auth_token = crate::keychain::get_api_key("noren-pro-token")
                .ok_or("Not logged in to Noren Pro. Go to Settings to sign in.")?;
            Box::new(noren_engine::NorenProxyClient::new(
                server_url,
                auth_token,
                format.clone(),
            ))
        } else {
            let api_key = if config.provider.requires_key {
                crate::keychain::get_api_key(&config.provider.keychain_id())
            } else {
                None
            };
            noren_engine::create_llm_client(&config, api_key).map_err(|e| e.to_string())?
        };
    let messages = vec![
        noren_engine::LlmMessage {
            role: noren_engine::Role::System,
            content: system_prompt,
        },
        noren_engine::LlmMessage {
            role: noren_engine::Role::User,
            content: user_content,
        },
    ];
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(4096),
    };

    let response = client
        .complete(&messages, &options)
        .await
        .map_err(|e| e.to_string())?;

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
    })
}

#[derive(Serialize)]
pub struct ComparisonResult {
    pub with_voice: GenerateResult,
    pub without_voice: GenerateResult,
}

#[tauri::command]
pub async fn generate_comparison(
    state: State<'_, AppState>,
    prompt: String,
    format: String,
    context: Option<String>,
    attachments: Option<Vec<String>>,
) -> Result<ComparisonResult, String> {
    // Generate WITH voice (guided enforcement)
    let with_voice = generate(
        state.clone(),
        prompt.clone(),
        format.clone(),
        "guided".to_string(),
        context.clone(),
        attachments.clone(),
    )
    .await?;

    // Generate WITHOUT voice — vanilla LLM, no profile
    let config = state.config.lock().unwrap().clone();

    let client: Box<dyn noren_engine::LlmClient> =
        if config.inference_mode == noren_engine::InferenceMode::NorenPro {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.noren.ink")
                .to_string();
            let auth_token = crate::keychain::get_api_key("noren-pro-token")
                .ok_or("Not logged in to Noren Pro.")?;
            Box::new(noren_engine::NorenProxyClient::new(
                server_url,
                auth_token,
                format.clone(),
            ))
        } else {
            let api_key = if config.provider.requires_key {
                crate::keychain::get_api_key(&config.provider.keychain_id())
            } else {
                None
            };
            noren_engine::create_llm_client(&config, api_key).map_err(|e| e.to_string())?
        };

    let mut user_content = match context.filter(|s| !s.is_empty()) {
        Some(ctx) => format!("Context:\n{}\n\nRequest: {}", ctx, prompt),
        None => prompt,
    };

    // Append file attachments
    if let Some(ref attached) = attachments {
        for (i, content) in attached.iter().enumerate() {
            user_content.push_str(&format!(
                "\n\n--- Attached document {} ---\n{}",
                i + 1,
                content
            ));
        }
    }

    let messages = vec![
        noren_engine::LlmMessage {
            role: noren_engine::Role::System,
            content: "You are a helpful writing assistant.".to_string(),
        },
        noren_engine::LlmMessage {
            role: noren_engine::Role::User,
            content: user_content,
        },
    ];
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(4096),
    };

    let response = client
        .complete(&messages, &options)
        .await
        .map_err(|e| e.to_string())?;

    let without_voice = GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
    };

    Ok(ComparisonResult {
        with_voice,
        without_voice,
    })
}

#[tauri::command]
pub fn list_formats(state: State<'_, AppState>) -> Vec<String> {
    let config = state.config.lock().unwrap();
    noren_engine::list_formats(&config.profile_dir)
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> noren_engine::Config {
    state.config.lock().unwrap().clone()
}
