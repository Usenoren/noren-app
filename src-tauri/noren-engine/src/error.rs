use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Missing required template variable: {{{{{0}}}}}")]
    MissingVariable(String),

    #[error("Prompt not found: {0}")]
    PromptNotFound(String),

    #[error("Missing API key for provider: {0}. Set it in your environment or ~/.noren/config.json")]
    MissingApiKey(String),

    #[error("Unknown provider: {0}")]
    UnknownProvider(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Profile error: {0}")]
    Profile(String),

    #[error("Prompt cache error: {0}")]
    PromptCache(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
