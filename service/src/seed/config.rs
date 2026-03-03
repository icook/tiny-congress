//! Configuration for the demo seed worker.

use serde::Deserialize;

/// Seed worker configuration, loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedConfig {
    /// `OpenRouter` API key
    pub openrouter_api_key: String,
    /// `OpenRouter` model identifier (e.g., "anthropic/claude-sonnet-4-6")
    #[serde(default = "default_model")]
    pub openrouter_model: String,
    /// Target number of open rooms with active polls
    #[serde(default = "default_target_rooms")]
    pub target_rooms: usize,
    /// Number of synthetic votes to cast per poll
    #[serde(default = "default_votes_per_poll")]
    pub votes_per_poll: usize,
    /// System prompt for LLM topic generation
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    /// Number of synthetic voter accounts to create
    #[serde(default = "default_voter_count")]
    pub voter_count: usize,
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4-6".to_string()
}

const fn default_target_rooms() -> usize {
    5
}

const fn default_votes_per_poll() -> usize {
    15
}

fn default_system_prompt() -> String {
    "You are a civic engagement topic generator. Generate realistic local governance topics \
     suitable for community polling. Topics should be specific, actionable, and relevant to \
     a small city or town. Examples: zoning changes, park improvements, transit priorities, \
     budget allocation, public safety initiatives."
        .to_string()
}

const fn default_voter_count() -> usize {
    20
}

impl SeedConfig {
    /// Load seed config from `SEED_*` environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if required env vars (like `SEED_OPENROUTER_API_KEY`) are missing.
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        use figment::{providers::Env, Figment};
        Figment::new()
            .merge(Env::prefixed("SEED_"))
            .extract()
            .map_err(Box::new)
    }
}
