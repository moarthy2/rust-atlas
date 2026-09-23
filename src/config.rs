//! Shared configuration loaded once at startup (zero per-request overhead).

use std::{env, time::Duration};

/// Runtime configuration. All fields are read once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub discord_token: String,
    pub application_id: Option<u64>,
    /// Custom OpenAI-compatible AI endpoint, e.g. `https://api.openai.com/v1`
    /// or `http://localhost:11434/v1` (Ollama).
    pub ai_base_url: String,
    pub ai_api_key: String,
    pub ai_model: String,
    /// Context7 API key (optional; enables fresh crate docs via context7).
    pub context7_api_key: Option<String>,
    /// Preload docs at startup? (uses more RAM at boot but faster first query).
    pub preload_docs: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let discord_token =
            env::var("DISCORD_TOKEN").map_err(|_| anyhow::anyhow!("DISCORD_TOKEN must be set"))?;
        Ok(Self {
            discord_token,
            application_id: env::var("APPLICATION_ID").ok().and_then(|v| v.parse().ok()),
            ai_base_url: env::var("AI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned()),
            ai_api_key: env::var("AI_API_KEY").unwrap_or_default(),
            ai_model: env::var("AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_owned()),
            context7_api_key: env::var("CONTEXT7_API_KEY").ok().filter(|s| !s.is_empty()),
            preload_docs: env::var("PRELOAD_DOCS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        })
    }

    /// Short timeout client hints.
    pub fn http_timeout() -> Duration {
        Duration::from_secs(20)
    }
}
