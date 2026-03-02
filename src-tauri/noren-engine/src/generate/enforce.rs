use std::collections::HashMap;
use regex::Regex;

use crate::error::EngineError;
use crate::template::fill_template;
use crate::types::EnforcementLevel;

/// Compose a system prompt for voice-matched text generation.
///
/// Loads the enforcement prompt (from cache or dev file), extracts the
/// system prompt template section, and fills it with the provided variables.
pub fn compose_system_prompt(
    enforcement_prompt: &str,
    core_identity: &str,
    context_layer: Option<&str>,
    format: &str,
    enforcement_level: &EnforcementLevel,
    user_request: &str,
) -> Result<String, EngineError> {
    // Try to extract system prompt template between ```\n and \n```
    let re = Regex::new(r"### System Prompt\s*\n\s*```\n([\s\S]*?)\n```").unwrap();

    if let Some(template_match) = re.captures(enforcement_prompt) {
        // Server/rich enforcement prompt — use template system
        let system_template = &template_match[1];

        let mut variables = HashMap::new();
        variables.insert("FORMAT".to_string(), format.to_string());
        variables.insert(
            "ENFORCEMENT_LEVEL".to_string(),
            enforcement_level.to_string(),
        );
        variables.insert("CORE_IDENTITY".to_string(), core_identity.to_string());
        variables.insert("USER_REQUEST".to_string(), user_request.to_string());

        if let Some(layer) = context_layer {
            variables.insert("CONTEXT_LAYER".to_string(), layer.to_string());
        }

        fill_template(system_template, &variables)
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

{{#if ENFORCEMENT_LEVEL == "strict"}}
Follow every pattern exactly. Output should be indistinguishable from the original.
{{/if}}
{{#if ENFORCEMENT_LEVEL == "guided"}}
Use profile as strong influence. Allow flexibility when no pattern applies.
{{/if}}
{{#if ENFORCEMENT_LEVEL == "light"}}
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
            &EnforcementLevel::Guided,
            "Write a tweet about focus",
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
            &EnforcementLevel::Strict,
            "Write a cold email",
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
            &EnforcementLevel::Light,
            "Write an essay",
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
            &EnforcementLevel::Guided,
            "Write a tweet about focus",
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
            &EnforcementLevel::Guided,
            "Write a tweet",
        )
        .unwrap();

        assert!(result.contains("My voice identity."));
        assert!(result.contains("Twitter context."));
        assert!(result.contains("Write a tweet"));
        assert!(result.contains("twitter"));
    }
}
