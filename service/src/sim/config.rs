//! Configuration for the simulation worker.

use serde::Deserialize;

/// Simulation worker configuration, loaded from `SIM_*` environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct SimConfig {
    /// Base URL of the `TinyCongress` API (e.g., `http://localhost:4000`)
    pub api_url: String,
    /// `OpenRouter` API key (not required when `mock_llm` is true)
    #[serde(default)]
    pub openrouter_api_key: String,
    /// Use deterministic mock content instead of calling `OpenRouter`
    #[serde(default)]
    pub mock_llm: bool,
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
    /// Log level filter (e.g., "info", "debug", "warn")
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Duration (in seconds) for polls created in sim rooms
    #[serde(default = "default_poll_duration_secs")]
    pub poll_duration_secs: i32,
    /// Room topic mode (e.g., "civic" or "`brand_ethics`")
    #[serde(default = "default_room_topic")]
    pub room_topic: String,
    /// Number of companies to curate from the S&P 500 (used in `brand_ethics` mode)
    #[serde(default = "default_company_count")]
    pub company_count: usize,
    /// Exa API key for evidence search (required for `brand_ethics` mode unless `mock_llm`)
    #[serde(default)]
    pub exa_api_key: String,
    /// Model for evidence synthesis step (default: Haiku for cost efficiency)
    #[serde(default = "default_evidence_model")]
    pub evidence_model: String,
    /// Dry run: only run LLM generation and write output to JSON file, skip API calls
    #[serde(default)]
    pub dry_run: bool,
    /// Battery test: path to a JSON file with model+search pairs to compare.
    /// Format: `[{"model": "anthropic/claude-sonnet-4-6", "search": true}, ...]`
    #[serde(default)]
    pub battery_config: Option<String>,
    /// Company name for battery test (e.g., "Sysco Corporation")
    #[serde(default)]
    pub battery_company: Option<String>,
    /// Ticker for battery test (e.g., "SYY")
    #[serde(default)]
    pub battery_ticker: Option<String>,
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4-6".to_string()
}

fn default_evidence_model() -> String {
    "deepseek/deepseek-v3.2".to_string()
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

fn default_log_level() -> String {
    "info".to_string()
}

const fn default_poll_duration_secs() -> i32 {
    86400 // 24 hours
}

fn default_room_topic() -> String {
    "civic".to_string()
}

const fn default_company_count() -> usize {
    25
}

impl SimConfig {
    /// Load sim config from `SIM_*` environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if the required env var `SIM_API_URL` is missing.
    /// `SIM_OPENROUTER_API_KEY` is optional when `SIM_MOCK_LLM=true`.
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        use figment::{providers::Env, Figment};
        Figment::new()
            .merge(Env::prefixed("SIM_"))
            .extract()
            .map_err(Box::new)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load_when_required_fields_present() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_API_URL", "http://localhost:4000");
            jail.set_env("SIM_OPENROUTER_API_KEY", "test-openrouter-key");

            let config = SimConfig::from_env().expect("should load with required fields set");

            assert_eq!(config.api_url, "http://localhost:4000");
            assert_eq!(config.openrouter_api_key, "test-openrouter-key");
            assert_eq!(config.openrouter_model, "anthropic/claude-sonnet-4-6");
            assert_eq!(config.target_rooms, 5);
            assert_eq!(config.votes_per_poll, 15);
            assert_eq!(
                config.system_prompt,
                "You are a civic engagement topic generator. Generate realistic local governance \
                 topics suitable for community polling. Topics should be specific, actionable, and \
                 relevant to a small city or town. Examples: zoning changes, park improvements, \
                 transit priorities, budget allocation, public safety initiatives."
            );
            assert_eq!(config.voter_count, 20);
            assert_eq!(config.log_level, "info");
            assert_eq!(config.poll_duration_secs, 86400);
            Ok(())
        });
    }

    #[test]
    fn error_when_api_url_missing() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_OPENROUTER_API_KEY", "key");

            let result = SimConfig::from_env();
            assert!(result.is_err(), "should fail without SIM_API_URL");
            Ok(())
        });
    }

    #[test]
    fn loads_without_api_key_for_mock_mode() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_API_URL", "http://localhost:4000");
            jail.set_env("SIM_MOCK_LLM", "true");

            let config = SimConfig::from_env().expect("should load without API key in mock mode");
            assert!(config.mock_llm);
            assert!(config.openrouter_api_key.is_empty());
            Ok(())
        });
    }

    #[test]
    fn error_when_all_required_fields_missing() {
        figment::Jail::expect_with(|_jail| {
            let result = SimConfig::from_env();
            assert!(result.is_err(), "should fail with no env vars set");
            Ok(())
        });
    }

    #[test]
    fn custom_values_override_defaults() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_API_URL", "https://api.example.com");
            jail.set_env("SIM_OPENROUTER_API_KEY", "ok");
            jail.set_env("SIM_OPENROUTER_MODEL", "openai/gpt-4o");
            jail.set_env("SIM_TARGET_ROOMS", "10");
            jail.set_env("SIM_VOTES_PER_POLL", "30");
            jail.set_env("SIM_SYSTEM_PROMPT", "Custom prompt");
            jail.set_env("SIM_VOTER_COUNT", "50");
            jail.set_env("SIM_LOG_LEVEL", "debug");

            let config = SimConfig::from_env().expect("should load with all fields set");

            assert_eq!(config.api_url, "https://api.example.com");
            assert_eq!(config.openrouter_model, "openai/gpt-4o");
            assert_eq!(config.target_rooms, 10);
            assert_eq!(config.votes_per_poll, 30);
            assert_eq!(config.system_prompt, "Custom prompt");
            assert_eq!(config.voter_count, 50);
            assert_eq!(config.log_level, "debug");
            Ok(())
        });
    }
}
