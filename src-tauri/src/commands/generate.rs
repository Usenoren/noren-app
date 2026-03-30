use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State, Window};

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

#[derive(Serialize, Deserialize, Clone)]
pub struct GenerateResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip)]
    pub voice_check: Option<VoiceCheckResult>,
    #[serde(skip)]
    pub routed_model: Option<String>,
    #[serde(skip)]
    pub route_reason: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct VoiceCheckResult {
    pub passed: bool,
    pub violations: Vec<noren_engine::generate::output_checks::Violation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<noren_engine::generate::output_checks::DensityCounts>,
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
    quick_action: Option<String>,
) -> Result<GenerateResult, String> {
    let config = state.config.lock().unwrap().clone();
    let mode = mode.as_deref().unwrap_or("generate");
    let pipeline = match pipeline.as_deref() {
        Some("light") => noren_engine::GenerationPipeline::LightEnforcement,
        _ => noren_engine::GenerationPipeline::Internalized,
    };

    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        generate_pro(&config, &prompt, &format, &level, mode, &pipeline, quick_action.as_deref(), context.as_deref(), attachments.as_deref(), None).await
    } else {
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
    quick_action: Option<&str>,
    context: Option<&str>,
    attachments: Option<&[String]>,
    generation_id: Option<&str>,
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
            quick_action,
            context,
            attachments,
            &options,
            generation_id,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        voice_check: None,
        routed_model: None,
        route_reason: None,
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

    // Load voice metadata for routing + output checks
    let metadata = noren_engine::load_voice_metadata(&config.profile_dir);

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

    // Voice routing: only override model when user is on the default Anthropic model.
    // If someone manually set Opus, respect their choice.
    let is_default_model = config.provider.model == "claude-sonnet-4-6";
    let (active_model, route_info) =
        if config.provider.provider_type == noren_engine::ProviderType::Anthropic && is_default_model {
            if let Some(ref meta) = metadata {
                let decision = noren_engine::generate::voice_router::route_voice_to_model(meta, format);
                if decision.model != config.provider.model {
                    (decision.model.clone(), Some((decision.model, decision.reason)))
                } else {
                    (config.provider.model.clone(), None)
                }
            } else {
                (config.provider.model.clone(), None)
            }
        } else {
            (config.provider.model.clone(), None)
        };

    let client: Box<dyn noren_engine::LlmClient> = {
        let api_key = if config.provider.requires_key {
            crate::keychain::get_api_key(&config.provider.keychain_id())
        } else {
            None
        };
        // Apply routed model override
        let mut effective_config = config.clone();
        effective_config.provider.model = active_model;
        noren_engine::create_llm_client(&effective_config, api_key).map_err(|e| e.to_string())?
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

    // Enable prompt caching for Anthropic BYOK: the system message (voice profile +
    // enforcement instructions) is identical across generation calls with the same profile.
    // Cached prefix tokens are 90% cheaper and process faster.
    let use_cache = config.provider.provider_type == noren_engine::ProviderType::Anthropic;
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: Some(8192),
        thinking,
        cache: if use_cache { Some(true) } else { None },
        ..Default::default()
    };

    let response = client
        .complete(&messages, &options)
        .await
        .map_err(|e| e.to_string())?;

    // Run output checks against the generated text
    let rhythm = metadata.as_ref().and_then(|m| {
        m.format_rhythms
            .as_ref()
            .and_then(|fr| fr.get(format))
            .or(m.baseline_rhythm.as_ref())
    });
    let checks = noren_engine::generate::output_checks::run_output_checks(
        &response.content,
        &core_identity,
        context_layer.map(String::as_str),
        rhythm,
    );

    let voice_check = Some(VoiceCheckResult {
        passed: checks.passed,
        violations: checks.violations,
        density: checks.density,
    });

    let (routed_model, route_reason) = match route_info {
        Some((model, reason)) => (Some(model), Some(reason)),
        None => (None, None),
    };

    Ok(GenerateResult {
        text: response.content,
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        voice_check,
        routed_model,
        route_reason,
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
        None,
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
        voice_check: None,
        routed_model: None,
        route_reason: None,
    };

    Ok(ComparisonResult {
        with_voice,
        without_voice,
    })
}

/// SSE event payloads emitted to the frontend window.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "delta")]
    Delta { text: String },
    #[serde(rename = "done")]
    Done {
        content: String,
        input_tokens: u64,
        output_tokens: u64,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        route_reason: Option<String>,
    },
    #[serde(rename = "cleanup_start")]
    CleanupStart,
    #[serde(rename = "cleanup_done")]
    CleanupDone {
        content: String,
        issues_found: u32,
        issues_fixed: u32,
        fix_spans: Vec<FixSpan>,
        #[serde(skip_serializing_if = "Option::is_none")]
        checks: Option<serde_json::Value>,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FixSpan {
    pub start: u32,
    pub end: u32,
}

/// Streaming generation for Pro mode. Emits events to the frontend window:
/// gen:delta, gen:done, gen:cleanup_start, gen:cleanup_done, gen:error.
///
/// BYOK path falls through to blocking generate (no streaming).
#[tauri::command]
pub async fn generate_stream(
    window: Window,
    state: State<'_, AppState>,
    prompt: String,
    format: String,
    level: String,
    mode: Option<String>,
    context: Option<String>,
    attachments: Option<Vec<String>>,
    generation_id: Option<String>,
) -> Result<(), String> {
    eprintln!("[generate_stream] called: prompt={:?} format={}", &prompt[..prompt.len().min(30)], format);
    // Reset cancellation flag at start
    state.cancel_generation.store(false, std::sync::atomic::Ordering::Relaxed);
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode != noren_engine::InferenceMode::NorenPro {
        // BYOK: fall back to blocking generate, emit done event
        let result = generate(
            state, prompt, format, level, mode, None, context, attachments, None,
        )
        .await?;
        let _ = window.emit(
            "gen:done",
            serde_json::json!({
                "type": "done",
                "content": result.text,
                "input_tokens": result.input_tokens,
                "output_tokens": result.output_tokens,
                "model": "byok"
            }),
        );
        return Ok(());
    }

    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .to_string();
    let auth_token = crate::keychain::get_api_key("noren-pro-token")
        .ok_or("Not logged in to Noren Pro. Go to Settings to sign in.")?;
    let refresh_token = crate::keychain::get_api_key("noren-pro-refresh");
    let mut client =
        noren_engine::NorenProxyClient::new(server_url, auth_token, format.clone());
    if let Some(rt) = refresh_token {
        client = client.with_token_refresh(rt, |new_access, new_refresh| {
            let _ = crate::keychain::store_api_key("noren-pro-token", &new_access);
            let _ = crate::keychain::store_api_key("noren-pro-refresh", &new_refresh);
        });
    }

    let pipeline_str = "internalized";
    let generation_mode = match mode.as_deref() {
        Some("adapt") => Some("adapt"),
        _ => Some("generate"),
    };
    let options = noren_engine::LlmOptions {
        temperature: Some(0.7),
        max_tokens: None,
        thinking: None,
        ..Default::default()
    };

    let resp = client
        .generate_server_composed_stream(
            &prompt,
            generation_id.as_deref(),
            &format,
            &level,
            Some(pipeline_str),
            generation_mode,
            context.as_deref(),
            attachments.as_deref(),
            &options,
        )
        .await
        .map_err(|e| e.to_string())?;

    // Read SSE stream line by line
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        // Check cancellation flag
        if state.cancel_generation.load(std::sync::atomic::Ordering::Relaxed) {
            state.cancel_generation.store(false, std::sync::atomic::Ordering::Relaxed);
            let _ = window.emit("gen:error", serde_json::json!({"type": "error", "message": "Generation cancelled"}));
            return Ok(());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE events (separated by \n\n)
        while let Some(pos) = buffer.find("\n\n") {
            let event_str = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            let trimmed = event_str.trim();
            if !trimmed.starts_with("data: ") {
                continue;
            }
            let json_str = &trimmed[6..];

            match serde_json::from_str::<StreamEvent>(json_str) {
                Ok(event) => {
                    let is_terminal = matches!(&event, StreamEvent::Done { .. } | StreamEvent::Error { .. });
                    let event_name = match &event {
                        StreamEvent::Delta { .. } => "gen:delta",
                        StreamEvent::Done { .. } => "gen:done",
                        StreamEvent::CleanupStart => "gen:cleanup_start",
                        StreamEvent::CleanupDone { .. } => "gen:cleanup_done",
                        StreamEvent::Error { .. } => "gen:error",
                    };
                    let _ = window.emit(event_name, &event);
                    // Don't wait for SSE connection to close — return immediately
                    // after done/error to unblock the frontend for follow-up requests.
                    if is_terminal {
                        return Ok(());
                    }
                }
                Err(_) => {
                    // Try as raw JSON value for forward compatibility
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(t) = val.get("type").and_then(|t| t.as_str()) {
                            let event_name = match t {
                                "delta" => "gen:delta",
                                "done" => "gen:done",
                                "cleanup_start" => "gen:cleanup_start",
                                "cleanup_done" => "gen:cleanup_done",
                                "error" => "gen:error",
                                _ => continue,
                            };
                            let _ = window.emit(event_name, &val);
                            if t == "done" || t == "error" {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Cancel an in-progress generation. Sets a flag the streaming loop checks.
#[tauri::command]
pub fn cancel_generation(state: State<'_, AppState>) {
    state
        .cancel_generation
        .store(true, std::sync::atomic::Ordering::Relaxed);
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

/// Rewrite a selected portion of text using voice-aware generation.
/// Returns only the rewritten selection text.
#[tauri::command]
pub async fn rewrite_selection(
    state: State<'_, AppState>,
    instruction: String,
    selection_text: String,
    full_text: String,
    format: String,
) -> Result<GenerateResult, String> {
    let prompt = format!(
        "Rewrite this in my voice: {}\n\n{}",
        instruction, selection_text
    );

    generate(
        state,
        prompt,
        format,
        "guided".to_string(),
        Some("adapt".to_string()),
        None,
        Some(full_text),
        None,
        Some("rewrite".to_string()),
    )
    .await
}
