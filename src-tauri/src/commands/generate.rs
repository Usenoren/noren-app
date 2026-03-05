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
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        // --- Pro path: server loads profile + composes prompt ---
        generate_pro(&config, &prompt, &format, &level, context.as_deref(), attachments.as_deref()).await
    } else {
        // --- BYOK path: client loads profile + composes prompt locally ---
        generate_byok(&config, state.encryption_key, &prompt, &format, &level, context.as_deref(), attachments.as_deref()).await
    }
}

/// Pro path — server handles profile + prompt composition + inference.
/// Client sends only { prompt, format, level }.
async fn generate_pro(
    config: &noren_engine::Config,
    prompt: &str,
    format: &str,
    level: &str,
    context: Option<&str>,
    attachments: Option<&[String]>,
) -> Result<GenerateResult, String> {
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.noren.ink")
        .to_string();
    let auth_token = crate::keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in to Noren Pro. Go to Settings to sign in.")?;

    let client = noren_engine::NorenProxyClient::new(server_url, auth_token, format.to_string());
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(4096),
        thinking: None,
    };

    let response = client
        .generate_server_composed(prompt, format, level, context, attachments, &options)
        .await
        .map_err(|e| e.to_string())?;

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
    })
}

/// BYOK path — client loads local profile, composes prompt, calls LLM directly.
async fn generate_byok(
    config: &noren_engine::Config,
    encryption_key: [u8; 32],
    prompt: &str,
    format: &str,
    level: &str,
    context: Option<&str>,
    attachments: Option<&[String]>,
) -> Result<GenerateResult, String> {
    // Load local profile — fall back to empty identity if none exists
    let (core_identity, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), std::collections::HashMap::new()));

    let context_layer = contexts.get(format);

    // Get enforcement prompt (cache → dev file → built-in fallback)
    let cache_dir = noren_engine::prompt_cache::default_cache_dir();
    let enforcement_prompt = noren_engine::prompt_cache::get_enforcement_prompt(
        &cache_dir,
        &encryption_key,
        config.server_url.as_deref(),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let enforcement_level = match level {
        "strict" => noren_engine::EnforcementLevel::Strict,
        "light" => noren_engine::EnforcementLevel::Light,
        _ => noren_engine::EnforcementLevel::Guided,
    };

    let system_prompt = noren_engine::compose_system_prompt(
        &enforcement_prompt,
        &core_identity,
        context_layer.map(String::as_str),
        format,
        &enforcement_level,
        prompt,
    )
    .map_err(|e| e.to_string())?;

    // Build user message
    let mut user_content = match context.filter(|s| !s.is_empty()) {
        Some(ctx) => std::format!("Context (selected text):\n{}\n\nRequest: {}", ctx, prompt),
        None => prompt.to_string(),
    };

    if let Some(attached) = attachments {
        for (i, content) in attached.iter().enumerate() {
            user_content.push_str(&std::format!(
                "\n\n--- Attached document {} ---\n{}",
                i + 1,
                content
            ));
        }
    }

    let client: Box<dyn noren_engine::LlmClient> = {
        let api_key = if config.provider.requires_key {
            crate::keychain::get_api_key(&config.provider.keychain_id())
        } else {
            None
        };
        noren_engine::create_llm_client(config, api_key).map_err(|e| e.to_string())?
    };

    let thinking = if config.extended_thinking {
        Some(noren_engine::ThinkingConfig {
            budget_tokens: config.thinking_budget,
        })
    } else {
        None
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
        max_tokens: Some(if config.extended_thinking { config.thinking_budget + 4096 } else { 4096 }),
        thinking,
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

    let mut user_content = match context.filter(|s| !s.is_empty()) {
        Some(ctx) => format!("Context:\n{}\n\nRequest: {}", ctx, prompt),
        None => prompt,
    };

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
        thinking: None,
    };

    // For "without voice", always use legacy messages path (even for Pro)
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
