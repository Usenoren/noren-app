use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

use super::LlmClient;
use crate::error::EngineError;
use crate::types::{LlmMessage, LlmOptions, LlmResponse, Role};

/// LLM client that proxies requests through the `claude` CLI.
///
/// This lets users leverage their Claude Pro/Max subscription for premium
/// models (Sonnet, Opus) via Claude Code's OAuth authentication, which has
/// access that raw OAuth tokens lack for direct API calls.
pub struct ClaudeCodeProxyClient {
    model: String,
}

/// Find the `claude` binary — Tauri apps don't inherit the user's shell PATH.
fn find_claude_binary() -> Result<PathBuf, EngineError> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let candidates = [
        format!("{}/.local/bin/claude", home),
        format!("{}/.claude/local/claude", home),
        "/usr/local/bin/claude".to_string(),
        "/opt/homebrew/bin/claude".to_string(),
    ];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }
    // Fallback: try bare name (works if PATH is set)
    Ok(PathBuf::from("claude"))
}

impl ClaudeCodeProxyClient {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[derive(Deserialize)]
struct ClaudeOutput {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    output_type: String,
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize, Default)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[async_trait]
impl LlmClient for ClaudeCodeProxyClient {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        options: &LlmOptions,
    ) -> Result<LlmResponse, EngineError> {
        // Build the user prompt from messages.
        // System messages become --system-prompt, user/assistant become the prompt body.
        let system_prompt = messages
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_prompt = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let prefix = match m.role {
                    Role::User => "",
                    Role::Assistant => "[Previous assistant response]\n",
                    Role::System => unreachable!(),
                };
                format!("{}{}", prefix, m.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        // Resolve model alias: map short names to what claude CLI expects
        let model_arg = match self.model.as_str() {
            m if m.contains("opus") => "opus",
            m if m.contains("haiku") => "haiku",
            _ => "sonnet", // default to sonnet
        };

        let claude_bin = find_claude_binary()?;
        let mut cmd = Command::new(&claude_bin);
        cmd.arg("--print")
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(model_arg)
            .arg("--no-session-persistence")
            .arg("--tools")
            .arg("")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !system_prompt.is_empty() {
            cmd.arg("--system-prompt").arg(&system_prompt);
        }

        if let Some(ref tc) = options.thinking {
            cmd.arg("--effort").arg("max");
            // Claude CLI doesn't expose budget_tokens directly,
            // but --effort max enables extended thinking.
            let _ = tc.budget_tokens; // acknowledge the field
        }

        // Pipe the user prompt via stdin
        cmd.arg("-"); // read from stdin

        let mut child = cmd.spawn().map_err(|e| {
            EngineError::Llm(format!(
                "Failed to spawn claude CLI. Is Claude Code installed? Error: {}",
                e
            ))
        })?;

        // Write prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(user_prompt.as_bytes()).await?;
            drop(stdin); // close stdin so claude knows input is done
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::Llm(format!(
                "claude CLI exited with {}: {}",
                output.status, stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        let parsed: ClaudeOutput = serde_json::from_str(&stdout).map_err(|e| {
            EngineError::Llm(format!(
                "Failed to parse claude CLI output: {}. Output: {}",
                e,
                &stdout[..stdout.len().min(500)]
            ))
        })?;

        if parsed.is_error || parsed.subtype == "error" {
            return Err(EngineError::Llm(format!(
                "claude CLI returned error: {}",
                parsed.result
            )));
        }

        let usage = parsed.usage.unwrap_or_default();

        Ok(LlmResponse {
            content: parsed.result,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        })
    }

    fn provider(&self) -> &str {
        "claude-code"
    }
}
