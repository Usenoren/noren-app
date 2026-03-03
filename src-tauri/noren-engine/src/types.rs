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
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    OpenaiCompatible,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::OpenaiCompatible => write!(f, "openai_compatible"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub model: String,
    /// Whether this provider requires an API key (false for local providers like Ollama)
    #[serde(rename = "requiresKey", default = "default_true")]
    pub requires_key: bool,
}

fn default_true() -> bool {
    true
}

impl ProviderConfig {
    pub fn keychain_id(&self) -> String {
        self.name.to_lowercase().replace(' ', "-")
    }
}

/// Preset provider configurations
impl ProviderConfig {
    pub fn anthropic() -> Self {
        Self {
            name: "anthropic".to_string(),
            provider_type: ProviderType::Anthropic,
            base_url: "https://api.anthropic.com/v1/messages".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            requires_key: true,
        }
    }

    pub fn openai() -> Self {
        Self {
            name: "openai".to_string(),
            provider_type: ProviderType::OpenaiCompatible,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            requires_key: true,
        }
    }

    pub fn gemini() -> Self {
        Self {
            name: "gemini".to_string(),
            provider_type: ProviderType::OpenaiCompatible,
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            model: "gemini-2.5-flash".to_string(),
            requires_key: true,
        }
    }

    pub fn ollama() -> Self {
        Self {
            name: "ollama".to_string(),
            provider_type: ProviderType::OpenaiCompatible,
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.1".to_string(),
            requires_key: false,
        }
    }

    pub fn custom(base_url: String, model: String, requires_key: bool) -> Self {
        Self {
            name: "custom".to_string(),
            provider_type: ProviderType::OpenaiCompatible,
            base_url,
            model,
            requires_key,
        }
    }

    pub fn preset_by_name(name: &str) -> Option<Self> {
        match name {
            "anthropic" => Some(Self::anthropic()),
            "openai" => Some(Self::openai()),
            "gemini" => Some(Self::gemini()),
            "ollama" => Some(Self::ollama()),
            _ => None,
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

// --- Inference mode ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InferenceMode {
    Byok,
    NorenPro,
}

impl Default for InferenceMode {
    fn default() -> Self {
        Self::Byok
    }
}

// --- Config ---

fn default_hotkey() -> String {
    "Meta+KeyK".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    #[serde(rename = "profileDir")]
    pub profile_dir: PathBuf,
    /// Server URL for fetching prompts and Noren Pro inference
    #[serde(rename = "serverUrl", skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// BYOK or Noren Pro
    #[serde(rename = "inferenceMode", default)]
    pub inference_mode: InferenceMode,
    /// Living profile opt-in (edit tracking + server analysis)
    #[serde(rename = "livingProfileEnabled", default)]
    pub living_profile_enabled: bool,
    /// Global hotkey string, e.g. "Meta+KeyK", "Meta+Shift+KeyN"
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs_home();
        Self {
            provider: ProviderConfig::anthropic(),
            profile_dir: home.join(".noren").join("profiles"),
            server_url: None,
            inference_mode: InferenceMode::Byok,
            living_profile_enabled: false,
            hotkey: default_hotkey(),
        }
    }
}

// --- Pipeline types ---

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
