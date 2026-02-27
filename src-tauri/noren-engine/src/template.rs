use regex::Regex;
use std::collections::HashMap;

use crate::error::EngineError;

/// Single-pass template variable replacement.
///
/// All `{{VAR}}` patterns are found in one regex scan. Each match is looked up
/// in the variables map and replaced via a closure. Content inside replaced
/// values is never re-scanned, preventing payload injection.
pub fn fill_template(
    template: &str,
    variables: &HashMap<String, String>,
) -> Result<String, EngineError> {
    // First, handle conditionals
    let processed = process_conditionals(template, variables);

    // Single-pass replacement for {{VAR}} placeholders.
    // Regex::replace_all with a closure naturally prevents re-scanning:
    // replaced content is appended directly to the output buffer without
    // being matched against the pattern again.
    let var_re = Regex::new(r"\{\{([A-Z_]+)\}\}").unwrap();

    let mut error: Option<EngineError> = None;
    let result = var_re.replace_all(&processed, |caps: &regex::Captures| {
        let var_name = &caps[1];
        match variables.get(var_name) {
            Some(value) => value.clone(),
            None => {
                if error.is_none() {
                    error = Some(EngineError::MissingVariable(var_name.to_string()));
                }
                // Return the original placeholder so we don't panic; error will be raised after
                format!("{{{{{}}}}}", var_name)
            }
        }
    });

    if let Some(err) = error {
        return Err(err);
    }

    Ok(result.into_owned())
}

/// Process conditionals in a template string.
///
/// Supports two forms:
/// 1. `{{#if VAR == "value"}}...{{/if}}` — equality check
/// 2. `{{#if VAR}}...{{/if}}` — truthy check (non-empty string)
fn process_conditionals(template: &str, variables: &HashMap<String, String>) -> String {
    // Handle {{#if VAR == "value"}}...{{/if}}
    let eq_re = Regex::new(r#"\{\{#if\s+([A-Z_]+)\s*==\s*"([^"]+)"\s*\}\}([\s\S]*?)\{\{/if\}\}"#)
        .unwrap();
    let result = eq_re.replace_all(template, |caps: &regex::Captures| {
        let var_name = &caps[1];
        let value = &caps[2];
        let content = &caps[3];
        if variables.get(var_name).map_or(false, |v| v == value) {
            content.to_string()
        } else {
            String::new()
        }
    });

    // Handle {{#if VAR}}...{{/if}} (truthy check)
    let truthy_re = Regex::new(r"\{\{#if\s+([A-Z_]+)\s*\}\}([\s\S]*?)\{\{/if\}\}").unwrap();
    let result = truthy_re.replace_all(&result, |caps: &regex::Captures| {
        let var_name = &caps[1];
        let content = &caps[2];
        if variables.get(var_name).map_or(false, |v| !v.is_empty()) {
            content.to_string()
        } else {
            String::new()
        }
    });

    result.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn replaces_simple_variables() {
        let template = "Hello {{NAME}}, you are {{AGE}} years old.";
        let result = fill_template(template, &vars(&[("NAME", "Alice"), ("AGE", "30")])).unwrap();
        assert_eq!(result, "Hello Alice, you are 30 years old.");
    }

    #[test]
    fn throws_on_missing_required_variable() {
        let template = "Hello {{NAME}}, your {{ROLE}} is ready.";
        let result = fill_template(template, &vars(&[("NAME", "Alice")]));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("{{ROLE}}"), "Error should mention ROLE: {}", err);
    }

    #[test]
    fn handles_equality_conditionals() {
        let template = "Start\n{{#if MODE == \"strict\"}}Strict content{{/if}}\n{{#if MODE == \"light\"}}Light content{{/if}}\nEnd";
        let result = fill_template(template, &vars(&[("MODE", "strict")])).unwrap();
        assert!(result.contains("Strict content"));
        assert!(!result.contains("Light content"));
    }

    #[test]
    fn handles_truthy_conditionals() {
        let template = "{{#if LAYER}}Has layer: {{LAYER}}{{/if}}";

        let result = fill_template(template, &vars(&[("LAYER", "twitter context")])).unwrap();
        assert!(result.contains("Has layer: twitter context"));

        let result = fill_template(template, &vars(&[])).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn injection_single_pass_prevents_payload_injection() {
        // If {{SAMPLES}} contains the literal string {{FORMAT}}, it should NOT be replaced
        let template = "Format: {{FORMAT}}\nSamples: {{SAMPLES}}";
        let result = fill_template(
            template,
            &vars(&[
                ("FORMAT", "twitter"),
                (
                    "SAMPLES",
                    "This tweet mentions {{FORMAT}} literally and {{UNKNOWN}} too",
                ),
            ]),
        )
        .unwrap();
        assert_eq!(
            result,
            "Format: twitter\nSamples: This tweet mentions {{FORMAT}} literally and {{UNKNOWN}} too"
        );
    }

    #[test]
    fn empty_variable_is_falsy_in_truthy_conditional() {
        let template = "{{#if LAYER}}Has layer{{/if}}No layer";
        let result = fill_template(template, &vars(&[("LAYER", "")])).unwrap();
        assert!(!result.contains("Has layer"));
        assert!(result.contains("No layer"));
    }
}
