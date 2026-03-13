use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::AppState;

const FORMAT_FAMILIES: &[&[&str]] = &[
    &["blog", "article", "newsletter", "essay"],
    &["tweet", "thread", "twitter"],
    &["email", "slack"],
    &["linkedin", "memo"],
];

/// Resolve a format to a context key using family fallback.
/// If the exact format exists in contexts, return its value directly.
/// Otherwise, find its family and return the first sibling that has a context.
fn resolve_context_format<'a>(
    format: &str,
    contexts: &'a HashMap<String, String>,
) -> Option<&'a String> {
    if let Some(ctx) = contexts.get(format) {
        return Some(ctx);
    }

    let family = FORMAT_FAMILIES.iter().find(|f| f.contains(&format));
    if let Some(family) = family {
        for sibling in *family {
            if let Some(ctx) = contexts.get(*sibling) {
                return Some(ctx);
            }
        }
    }

    None
}

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
    mode: Option<String>,
    pipeline: Option<String>,
    context: Option<String>,
    attachments: Option<Vec<String>>,
) -> Result<GenerateResult, String> {
    let config = state.config.lock().unwrap().clone();
    let mode = mode.as_deref().unwrap_or("generate");
    let pipeline = match pipeline.as_deref() {
        Some("light") => noren_engine::GenerationPipeline::LightEnforcement,
        _ => noren_engine::GenerationPipeline::Internalized,
    };

    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        // --- Pro path: server loads profile + composes prompt ---
        generate_pro(&config, &prompt, &format, &level, mode, &pipeline, context.as_deref(), attachments.as_deref()).await
    } else {
        // --- BYOK path: client loads profile + composes prompt locally ---
        generate_byok(&config, state.encryption_key, &prompt, &format, &level, mode, &pipeline, context.as_deref(), attachments.as_deref()).await
    }
}

/// Pro path — server handles profile + prompt composition + inference.
/// Client sends { prompt, format, level, mode, generation_mode }.
async fn generate_pro(
    config: &noren_engine::Config,
    prompt: &str,
    format: &str,
    level: &str,
    mode: &str,
    pipeline: &noren_engine::GenerationPipeline,
    context: Option<&str>,
    attachments: Option<&[String]>,
) -> Result<GenerateResult, String> {
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .to_string();
    let auth_token = crate::keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in to Noren Pro. Go to Settings to sign in.")?;

    let refresh_token = crate::keychain::get_api_key("noren-pro-refresh");
    let mut client = noren_engine::NorenProxyClient::new(server_url, auth_token, format.to_string());
    if let Some(rt) = refresh_token {
        client = client.with_token_refresh(rt, |new_access, new_refresh| {
            let _ = crate::keychain::store_api_key("noren-pro-token", &new_access);
            let _ = crate::keychain::store_api_key("noren-pro-refresh", &new_refresh);
        });
    }

    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: None,
        thinking: None,
        ..Default::default()
    };

    let pipeline_str = match pipeline {
        noren_engine::GenerationPipeline::LightEnforcement => "light",
        noren_engine::GenerationPipeline::Internalized => "internalized",
    };
    let generation_mode = match mode {
        "adapt" => Some("adapt"),
        _ => Some("generate"),
    };

    let response = client
        .generate_server_composed(
            prompt,
            format,
            level,
            Some(pipeline_str),
            generation_mode,
            context,
            attachments,
            &options,
        )
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
    mode: &str,
    pipeline: &noren_engine::GenerationPipeline,
    context: Option<&str>,
    attachments: Option<&[String]>,
) -> Result<GenerateResult, String> {
    // Load local profile — fall back to empty identity if none exists
    let (core_identity, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), std::collections::HashMap::new()));

    let context_layer = resolve_context_format(format, &contexts);

    // Load calibration data if available
    let calibration = noren_engine::load_calibration(&config.profile_dir);

    // Get the appropriate template based on pipeline
    let cache_dir = noren_engine::prompt_cache::default_cache_dir();
    let template_prompt = match pipeline {
        noren_engine::GenerationPipeline::Internalized => {
            noren_engine::prompt_cache::get_internalized_prompt(
                &cache_dir,
                &encryption_key,
                config.server_url.as_deref(),
                None,
            )
            .await
        }
        noren_engine::GenerationPipeline::LightEnforcement => {
            noren_engine::prompt_cache::get_enforcement_prompt(
                &cache_dir,
                &encryption_key,
                config.server_url.as_deref(),
                None,
            )
            .await
        }
    }
    .map_err(|e| e.to_string())?;

    // Frontend sends "strict"/"guided"/"light"; map to engine's faithful/balanced/loose
    let enforcement_level = match level {
        "strict" => noren_engine::EnforcementLevel::Faithful,
        "light" => noren_engine::EnforcementLevel::Loose,
        _ => noren_engine::EnforcementLevel::Balanced,
    };

    let system_prompt = noren_engine::compose_system_prompt(
        &template_prompt,
        &core_identity,
        context_layer.map(String::as_str),
        format,
        &enforcement_level,
        prompt,
        mode,
        calibration.as_ref(),
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

    // Auto-enable thinking for internalized on Anthropic, matching CLI behavior
    let use_thinking = match pipeline {
        noren_engine::GenerationPipeline::Internalized
            if config.provider.provider_type == noren_engine::ProviderType::Anthropic =>
        {
            true
        }
        _ => config.extended_thinking,
    };

    let thinking_budget = config.thinking_budget; // default 10000

    let thinking = if use_thinking {
        Some(noren_engine::ThinkingConfig {
            budget_tokens: thinking_budget,
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
        max_tokens: Some(8192),
        thinking,
        ..Default::default()
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
    // Generate WITH voice (balanced enforcement, default pipeline)
    let with_voice = generate(
        state.clone(),
        prompt.clone(),
        format.clone(),
        "balanced".to_string(),
        None,
        None,
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
        max_tokens: Some(8192),
        thinking: None,
        ..Default::default()
    };

    // For "without voice", always use legacy messages path (even for Pro)
    let client: Box<dyn noren_engine::LlmClient> =
        if config.inference_mode == noren_engine::InferenceMode::NorenPro {
            let server_url = config
                .server_url
                .as_deref()
                .unwrap_or("https://api.usenoren.ai")
                .to_string();
            let auth_token = crate::keychain::get_api_key("noren-pro-token")
                .ok_or("Not logged in to Noren Pro.")?;
            let refresh_token = crate::keychain::get_api_key("noren-pro-refresh");
            let mut proxy = noren_engine::NorenProxyClient::new(
                server_url,
                auth_token,
                format.clone(),
            );
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
