use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

const FORMAT_FAMILIES: &[&[&str]] = &[
    &["blog", "article", "newsletter", "essay"],
    &["tweet", "thread", "twitter"],
    &["email", "slack"],
    &["linkedin", "memo"],
];

const FORMAT_MAX_TOKENS: &[(&str, u32)] = &[
    ("tweet", 256),
    ("twitter", 256),
    ("thread", 4096),
    ("email", 2048),
    ("slack", 2048),
    ("linkedin", 1024),
    ("memo", 1024),
    ("blog", 8192),
    ("article", 8192),
    ("essay", 8192),
    ("newsletter", 8192),
    ("longform", 8192),
];
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Repurpose prompt template (mirrors prompts/30-repurpose.md).
/// Embedded here so BYOK path works without loading from disk.
const REPURPOSE_TEMPLATE: &str = r#"You are going to write as a specific person. Their voice profile is below.

{{CORE_IDENTITY}}

{{#if CALIBRATION}}
### Voice Calibration

When the profile doesn't clearly specify a stylistic choice, use these
user-stated preferences as tie-breakers:

{{CALIBRATION}}
{{/if}}

{{#if CONTEXT_LAYER}}

{{CONTEXT_LAYER}}
{{/if}}

The user provides content originally written as a {{SOURCE_FORMAT}}. Transform it into
{{FORMAT}} content. Capture the key ideas but follow the {{FORMAT}} conventions entirely.
Do not preserve the source structure. Write as if creating original {{FORMAT}} content
about these ideas.

Do not copy the example quotes from the profile into your output. Do not use the
anti-pattern words listed in the profile. Follow the format conventions in the profile.
Output the text only, no meta-commentary."#;

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

fn max_tokens_for_format(format: &str) -> u32 {
    FORMAT_MAX_TOKENS
        .iter()
        .find(|(f, _)| *f == format)
        .map(|(_, t)| *t)
        .unwrap_or(DEFAULT_MAX_TOKENS)
}

fn compose_repurpose_prompt(
    core_identity: &str,
    context_layer: Option<&str>,
    source_format: &str,
    target_format: &str,
    calibration: Option<&noren_engine::CalibrationData>,
) -> Result<String, String> {
    let mut variables = HashMap::new();
    variables.insert("FORMAT".to_string(), target_format.to_string());
    variables.insert("SOURCE_FORMAT".to_string(), source_format.to_string());
    variables.insert("CORE_IDENTITY".to_string(), core_identity.to_string());

    if let Some(layer) = context_layer {
        variables.insert("CONTEXT_LAYER".to_string(), layer.to_string());
    }

    if let Some(cal) = calibration {
        if !cal.sentence_pairs.is_empty() {
            let lines: Vec<String> = cal
                .sentence_pairs
                .iter()
                .map(|pair| {
                    let (chosen, rejected) = if pair.selected == "A" {
                        (&pair.option_a, &pair.option_b)
                    } else {
                        (&pair.option_b, &pair.option_a)
                    };
                    format!(
                        "- **{}**: Prefer \"{}\" over \"{}\"",
                        pair.dimension, chosen, rejected
                    )
                })
                .collect();
            variables.insert("CALIBRATION".to_string(), lines.join("\n"));
        }
    }

    noren_engine::fill_template(REPURPOSE_TEMPLATE, &variables).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct RepurposeFormatResult {
    pub format: String,
    pub content: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub passed: bool,
}

#[derive(Serialize)]
pub struct RepurposeResult {
    pub results: Vec<RepurposeFormatResult>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

#[derive(Deserialize)]
struct ServerRepurposeFormatResult {
    format: String,
    content: String,
    input_tokens: u64,
    output_tokens: u64,
    passed: bool,
}

#[derive(Deserialize)]
struct ServerRepurposeResponse {
    results: Vec<ServerRepurposeFormatResult>,
    total_input_tokens: u64,
    total_output_tokens: u64,
}

#[tauri::command]
pub async fn repurpose(
    state: State<'_, AppState>,
    source_content: String,
    source_format: String,
    target_formats: Option<Vec<String>>,
) -> Result<RepurposeResult, String> {
    let config = state.config.lock().unwrap().clone();

    if config.inference_mode == noren_engine::InferenceMode::NorenPro {
        repurpose_pro(
            &config,
            &source_content,
            &source_format,
            target_formats.as_deref(),
        )
        .await
    } else {
        repurpose_byok(
            &config,
            state.encryption_key,
            &source_content,
            &source_format,
            target_formats.as_deref(),
        )
        .await
    }
}

async fn repurpose_pro(
    config: &noren_engine::Config,
    source_content: &str,
    source_format: &str,
    target_formats: Option<&[String]>,
) -> Result<RepurposeResult, String> {
    let server_url = config
        .server_url
        .as_deref()
        .unwrap_or("https://api.usenoren.ai")
        .to_string();

    let body = serde_json::json!({
        "source_content": source_content,
        "source_format": source_format,
        "target_formats": target_formats,
    });
    let repurpose_url = format!("{}/v1/repurpose/", server_url);
    let resp = crate::auth_client::authed_request(&server_url, |client, token| {
        client
            .post(&repurpose_url)
            .json(&body)
            .header("Authorization", format!("Bearer {}", token))
    })
    .await
    .map_err(crate::auth_client::normalize_auth_error)?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Server error: {}", text));
    }

    let data: ServerRepurposeResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(RepurposeResult {
        total_input_tokens: data.total_input_tokens,
        total_output_tokens: data.total_output_tokens,
        results: data
            .results
            .into_iter()
            .map(|r| RepurposeFormatResult {
                format: r.format,
                content: r.content,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                passed: r.passed,
            })
            .collect(),
    })
}

async fn repurpose_byok(
    config: &noren_engine::Config,
    _encryption_key: [u8; 32],
    source_content: &str,
    source_format: &str,
    target_formats: Option<&[String]>,
) -> Result<RepurposeResult, String> {
    // Load local profile
    let (core_identity, contexts) = noren_engine::load_profile(&config.profile_dir)
        .unwrap_or_else(|_| (String::new(), HashMap::new()));

    let calibration = noren_engine::load_calibration(&config.profile_dir);

    // Resolve target formats
    let targets: Vec<String> = if let Some(specified) = target_formats {
        specified.to_vec()
    } else {
        let source_family = FORMAT_FAMILIES.iter().find(|f| f.contains(&source_format));
        let exclude: std::collections::HashSet<&str> = match source_family {
            Some(fam) => fam.iter().copied().collect(),
            None => [source_format].into_iter().collect(),
        };
        contexts
            .keys()
            .filter(|k| !exclude.contains(k.as_str()))
            .cloned()
            .collect()
    };

    if targets.is_empty() {
        return Err("No target formats available.".to_string());
    }

    // Create LLM client
    let api_key = if config.provider.requires_key {
        crate::keychain::get_api_key(&config.provider.keychain_id())
    } else {
        None
    };
    let client: Box<dyn noren_engine::LlmClient> =
        noren_engine::create_llm_client(config, api_key).map_err(|e| e.to_string())?;

    let use_cache = config.provider.provider_type == noren_engine::ProviderType::Anthropic;

    // Run targets sequentially (prompt caching still applies at the API level,
    // and the LlmClient trait object can't be shared across tokio tasks).
    let mut results = Vec::new();
    let mut total_input = 0u64;
    let mut total_output = 0u64;

    for target in &targets {
        let context_layer = resolve_context_format(target.as_str(), &contexts);
        let system_prompt = compose_repurpose_prompt(
            &core_identity,
            context_layer.map(String::as_str),
            source_format,
            target.as_str(),
            calibration.as_ref(),
        )?;

        let max_tokens = max_tokens_for_format(target.as_str());
        let messages = vec![
            noren_engine::LlmMessage {
                role: noren_engine::Role::System,
                content: system_prompt,
            },
            noren_engine::LlmMessage {
                role: noren_engine::Role::User,
                content: source_content.to_string(),
            },
        ];

        let options = noren_engine::LlmOptions {
            temperature: Some(0.7),
            max_tokens: Some(max_tokens as u32),
            thinking: None,
            cache: if use_cache { Some(true) } else { None },
            ..Default::default()
        };

        match client.complete(&messages, &options).await {
            Ok(response) => {
                total_input += response.input_tokens;
                total_output += response.output_tokens;
                results.push(RepurposeFormatResult {
                    format: target.clone(),
                    content: response.content,
                    input_tokens: response.input_tokens,
                    output_tokens: response.output_tokens,
                    passed: true, // BYOK skips output checks for speed
                });
            }
            Err(e) => {
                eprintln!("[repurpose] format {} failed: {}", target, e);
            }
        }
    }

    if results.is_empty() {
        return Err("All target format generations failed.".to_string());
    }

    Ok(RepurposeResult {
        results,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
    })
}
