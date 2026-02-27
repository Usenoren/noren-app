use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// --- LLM types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct LlmOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// --- Provider ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
    Gemini,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Anthropic => write!(f, "anthropic"),
            Provider::OpenAI => write!(f, "openai"),
            Provider::Gemini => write!(f, "gemini"),
        }
    }
}

impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::OpenAI),
            "gemini" => Ok(Provider::Gemini),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

// --- Enforcement level ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EnforcementLevel {
    Strict,
    Guided,
    Light,
}

impl std::fmt::Display for EnforcementLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnforcementLevel::Strict => write!(f, "strict"),
            EnforcementLevel::Guided => write!(f, "guided"),
            EnforcementLevel::Light => write!(f, "light"),
        }
    }
}

// --- Config ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: Provider,
    pub model: String,
    #[serde(rename = "profileDir")]
    pub profile_dir: PathBuf,
    #[serde(rename = "anthropicApiKey", skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(rename = "openaiApiKey", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(rename = "geminiApiKey", skip_serializing_if = "Option::is_none")]
    pub gemini_api_key: Option<String>,
    /// Server URL for fetching prompts
    #[serde(rename = "serverUrl", skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs_home();
        Self {
            provider: Provider::Anthropic,
            model: "claude-sonnet-4-20250514".to_string(),
            profile_dir: home.join(".noren").join("profiles"),
            anthropic_api_key: None,
            openai_api_key: None,
            gemini_api_key: None,
            server_url: None,
        }
    }
}

// --- Pipeline types (for future milestones, defined here for completeness) ---

#[derive(Debug, Clone)]
pub struct FormatGroup {
    pub format: String,
    pub samples: String,
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub output: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProgress {
    pub step: u32,
    pub total_steps: u32,
    pub label: String,
    pub status: ProgressStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProgressStatus {
    Running,
    Done,
    Failed,
}

// --- Profile info ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub path: String,
    pub name: String,
    pub formats: Vec<String>,
}

// --- Helpers ---

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Profile content returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileContent {
    pub core_identity: String,
    pub contexts: HashMap<String, String>,
    pub quality_check: Option<String>,
}
