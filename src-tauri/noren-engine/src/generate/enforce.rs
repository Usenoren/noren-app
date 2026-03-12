use std::collections::HashMap;
use regex::Regex;

use crate::error::EngineError;
use crate::template::fill_template;
use crate::types::{CalibrationData, EnforcementLevel};

/// Format calibration pairs as template-insertable text.
fn format_calibration(calibration: &CalibrationData) -> Option<String> {
    if calibration.sentence_pairs.is_empty() {
        return None;
    }

    let lines: Vec<String> = calibration
        .sentence_pairs
        .iter()
        .map(|pair| {
            let (chosen, rejected) = if pair.selected == "A" {
                (&pair.option_a, &pair.option_b)
            } else {
                (&pair.option_b, &pair.option_a)
            };
            format!("- **{}**: Prefer \"{}\" over \"{}\"", pair.dimension, chosen, rejected)
        })
        .collect();

    Some(lines.join("\n"))
}

/// Compose a system prompt for voice-matched text generation.
///
/// Loads the template prompt (internalized or enforcement), extracts the
/// system prompt template section, and fills it with the provided variables.
pub fn compose_system_prompt(
    template_prompt: &str,
    core_identity: &str,
    context_layer: Option<&str>,
    format: &str,
    enforcement_level: &EnforcementLevel,
    user_request: &str,
    mode: &str,
    calibration: Option<&CalibrationData>,
) -> Result<String, EngineError> {
    // Try to extract system prompt template between ```\n and \n```
    let re = Regex::new(r"### System Prompt\s*\n\s*```\n([\s\S]*?)\n```").unwrap();

    if let Some(template_match) = re.captures(template_prompt) {
        // Server/rich template — use template system
        let system_template = &template_match[1];

        let mut variables = HashMap::new();
        variables.insert("FORMAT".to_string(), format.to_string());
        variables.insert(
            "ENFORCEMENT_LEVEL".to_string(),
            enforcement_level.to_string(),
        );
        variables.insert("CORE_IDENTITY".to_string(), core_identity.to_string());
        variables.insert("USER_REQUEST".to_string(), user_request.to_string());
        variables.insert("MODE".to_string(), mode.to_string());

        if let Some(layer) = context_layer {
            variables.insert("CONTEXT_LAYER".to_string(), layer.to_string());
        }

        let cal_text = calibration.and_then(format_calibration);

        if let Some(ref text) = cal_text {
            variables.insert("CALIBRATION".to_string(), text.clone());
        }

        let mut result = fill_template(system_template, &variables)?;

        // If the template didn't consume {{CALIBRATION}} (e.g. light enforcement),
        // append it after template filling, matching CLI behavior.
        if let Some(text) = cal_text {
            if !result.contains(&text) {
                result.push_str(&format!(
                    "\n\n## Voice Calibration\n\nWhen the voice profile doesn't clearly specify a stylistic choice, use these user-stated preferences as tie-breakers:\n\n{}",
                    text
                ));
            }
        }

        Ok(result)
    } else {
        // Fallback — compose a direct system prompt (standalone/free mode)
        let mut prompt = format!(
            "You are a writing assistant. Write {} content in the voice described below.\n\n{}",
            format, core_identity
        );
        if let Some(layer) = context_layer {
            prompt.push_str(&format!("\n\n{}", layer));
        }
        prompt.push_str(&format!("\n\n{}\n\nWrite only the final text. No commentary.", user_request));
        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_enforcement_prompt() -> String {
        r#"# Enforcement Prompt

Some preamble text.

### System Prompt

```
You are writing {{FORMAT}} content.
Enforcement level: {{ENFORCEMENT_LEVEL}}

## Core Identity
{{CORE_IDENTITY}}

{{#if CONTEXT_LAYER}}
## Context Layer
{{CONTEXT_LAYER}}
{{/if}}

{{#if ENFORCEMENT_LEVEL == "faithful"}}
Follow every pattern exactly. Output should be indistinguishable from the original.
{{/if}}
{{#if ENFORCEMENT_LEVEL == "balanced"}}
Use profile as strong influence. Allow flexibility when no pattern applies.
{{/if}}
{{#if ENFORCEMENT_LEVEL == "loose"}}
Core identity only. Format structure is relaxed.
{{/if}}

User wants to write: {{USER_REQUEST}}
```

Some footer text.
"#
        .to_string()
    }

    #[test]
    fn compose_guided_with_context() {
        let prompt = mock_enforcement_prompt();
        let result = compose_system_prompt(
            &prompt,
            "I use medical analogies.",
            Some("Short punchy tweets."),
            "twitter",
            &EnforcementLevel::Balanced,
            "Write a tweet about focus",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("writing twitter content"));
        assert!(result.contains("I use medical analogies."));
        assert!(result.contains("Short punchy tweets."));
        assert!(result.contains("strong influence"));
        assert!(!result.contains("every pattern exactly"));
        assert!(result.contains("Write a tweet about focus"));
    }

    #[test]
    fn compose_strict_without_context() {
        let prompt = mock_enforcement_prompt();
        let result = compose_system_prompt(
            &prompt,
            "Core identity here.",
            None,
            "email",
            &EnforcementLevel::Faithful,
            "Write a cold email",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("writing email content"));
        assert!(result.contains("Core identity here."));
        assert!(!result.contains("Context Layer"));
        assert!(result.contains("every pattern exactly"));
    }

    #[test]
    fn compose_light() {
        let prompt = mock_enforcement_prompt();
        let result = compose_system_prompt(
            &prompt,
            "Core identity.",
            None,
            "longform",
            &EnforcementLevel::Loose,
            "Write an essay",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("structure is relaxed"));
    }

    #[test]
    fn builtin_prompt_works() {
        let builtin = crate::prompt_cache::BUILTIN_ENFORCEMENT_PROMPT;
        let result = compose_system_prompt(
            builtin,
            "I write casually and directly.",
            None,
            "general",
            &EnforcementLevel::Balanced,
            "Write a tweet about focus",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("I write casually and directly."));
        assert!(result.contains("Write a tweet about focus"));
    }

    #[test]
    fn fallback_when_template_unparseable() {
        let broken_prompt = "This has no template markers at all.";
        let result = compose_system_prompt(
            broken_prompt,
            "My voice identity.",
            Some("Twitter context."),
            "twitter",
            &EnforcementLevel::Balanced,
            "Write a tweet",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("My voice identity."));
        assert!(result.contains("Twitter context."));
        assert!(result.contains("Write a tweet"));
        assert!(result.contains("twitter"));
    }

    #[test]
    fn builtin_internalized_prompt_works() {
        let builtin = crate::prompt_cache::BUILTIN_INTERNALIZED_PROMPT;
        let result = compose_system_prompt(
            builtin,
            "I write with dry humor and short sentences.",
            Some("Punchy tweets, no hashtags."),
            "twitter",
            &EnforcementLevel::Balanced, // ignored by internalized template
            "Write a tweet about coffee",
            "generate",
            None,
        )
        .unwrap();

        assert!(result.contains("I write with dry humor and short sentences."));
        assert!(result.contains("Punchy tweets, no hashtags."));
        assert!(result.contains("WHAT TO WRITE"));
        assert!(!result.contains("WHAT TO RESTYLE"));
    }

    #[test]
    fn internalized_adapt_mode() {
        let builtin = crate::prompt_cache::BUILTIN_INTERNALIZED_PROMPT;
        let result = compose_system_prompt(
            builtin,
            "Core identity.",
            None,
            "email",
            &EnforcementLevel::Balanced,
            "Restyle this email",
            "adapt",
            None,
        )
        .unwrap();

        assert!(result.contains("WHAT TO RESTYLE"));
        assert!(!result.contains("WHAT TO WRITE"));
    }

    #[test]
    fn calibration_injected_into_template() {
        let builtin = crate::prompt_cache::BUILTIN_INTERNALIZED_PROMPT;
        let calibration = crate::types::CalibrationData {
            source: "guided".to_string(),
            domain: "tech".to_string(),
            writing_format: "blog".to_string(),
            sentence_pairs: vec![
                crate::types::CalibrationPair {
                    dimension: "Formality".to_string(),
                    selected: "A".to_string(),
                    option_a: "casual tone".to_string(),
                    option_b: "formal tone".to_string(),
                },
                crate::types::CalibrationPair {
                    dimension: "Humor".to_string(),
                    selected: "B".to_string(),
                    option_a: "dry delivery".to_string(),
                    option_b: "playful wit".to_string(),
                },
            ],
            completed_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
        };

        let result = compose_system_prompt(
            builtin,
            "Core identity.",
            None,
            "blog",
            &EnforcementLevel::Balanced,
            "Write a blog post",
            "generate",
            Some(&calibration),
        )
        .unwrap();

        assert!(result.contains("Voice Calibration"));
        assert!(result.contains("Prefer \"casual tone\" over \"formal tone\""));
        assert!(result.contains("Prefer \"playful wit\" over \"dry delivery\""));
    }

    #[test]
    fn calibration_appended_for_light_enforcement() {
        // The light enforcement builtin has no {{CALIBRATION}} block,
        // so calibration must be appended after template filling.
        let builtin = crate::prompt_cache::BUILTIN_ENFORCEMENT_PROMPT;
        let calibration = crate::types::CalibrationData {
            source: "guided".to_string(),
            domain: "tech".to_string(),
            writing_format: "blog".to_string(),
            sentence_pairs: vec![
                crate::types::CalibrationPair {
                    dimension: "Formality".to_string(),
                    selected: "A".to_string(),
                    option_a: "casual tone".to_string(),
                    option_b: "formal tone".to_string(),
                },
            ],
            completed_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
        };

        let result = compose_system_prompt(
            builtin,
            "Core identity.",
            None,
            "blog",
            &EnforcementLevel::Balanced,
            "Write a blog post",
            "generate",
            Some(&calibration),
        )
        .unwrap();

        assert!(result.contains("Voice Calibration"));
        assert!(result.contains("Prefer \"casual tone\" over \"formal tone\""));
    }

    #[test]
    fn empty_calibration_omitted() {
        let builtin = crate::prompt_cache::BUILTIN_INTERNALIZED_PROMPT;
        let calibration = crate::types::CalibrationData {
            source: "guided".to_string(),
            domain: "tech".to_string(),
            writing_format: "blog".to_string(),
            sentence_pairs: vec![],
            completed_at: "2026-01-01T00:00:00Z".to_string(),
            version: 1,
        };

        let result = compose_system_prompt(
            builtin,
            "Core identity.",
            None,
            "blog",
            &EnforcementLevel::Balanced,
            "Write a blog post",
            "generate",
            Some(&calibration),
        )
        .unwrap();

        assert!(!result.contains("Voice Calibration"));
    }
}
