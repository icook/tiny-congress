//! Configuration for the simulation worker.

use serde::Deserialize;

/// Simulation worker configuration, loaded from `SIM_*` environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct SimConfig {
    /// Base URL of the `TinyCongress` API (e.g., `http://localhost:4000`)
    pub api_url: String,
    /// API key for the verifier service
    pub verifier_api_key: String,
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
    /// Log level filter (e.g., "info", "debug", "warn")
    #[serde(default = "default_log_level")]
    pub log_level: String,
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

fn default_log_level() -> String {
    "info".to_string()
}

impl SimConfig {
    /// Load sim config from `SIM_*` environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if required env vars (`SIM_API_URL`,
    /// `SIM_VERIFIER_API_KEY`, `SIM_OPENROUTER_API_KEY`) are missing.
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
            jail.set_env("SIM_VERIFIER_API_KEY", "test-verifier-key");
            jail.set_env("SIM_OPENROUTER_API_KEY", "test-openrouter-key");

            let config = SimConfig::from_env().expect("should load with required fields set");

            assert_eq!(config.api_url, "http://localhost:4000");
            assert_eq!(config.verifier_api_key, "test-verifier-key");
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
            Ok(())
        });
    }

    #[test]
    fn error_when_api_url_missing() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_VERIFIER_API_KEY", "key");
            jail.set_env("SIM_OPENROUTER_API_KEY", "key");

            let result = SimConfig::from_env();
            assert!(result.is_err(), "should fail without SIM_API_URL");
            Ok(())
        });
    }

    #[test]
    fn error_when_verifier_api_key_missing() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_API_URL", "http://localhost:4000");
            jail.set_env("SIM_OPENROUTER_API_KEY", "key");

            let result = SimConfig::from_env();
            assert!(result.is_err(), "should fail without SIM_VERIFIER_API_KEY");
            Ok(())
        });
    }

    #[test]
    fn error_when_openrouter_api_key_missing() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SIM_API_URL", "http://localhost:4000");
            jail.set_env("SIM_VERIFIER_API_KEY", "key");

            let result = SimConfig::from_env();
            assert!(
                result.is_err(),
                "should fail without SIM_OPENROUTER_API_KEY"
            );
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
            jail.set_env("SIM_VERIFIER_API_KEY", "vk");
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
