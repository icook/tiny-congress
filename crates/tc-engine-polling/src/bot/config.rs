//! Bot configuration types deserialized from `rooms__rooms.engine_config.bot`.

use serde::Deserialize;

// ─── Default value helpers ───────────────────────────────────────────────────

const fn default_enabled() -> bool {
    false
}

const fn default_run_mode() -> RunMode {
    RunMode::Iterate
}

const fn default_schedule_secs() -> u64 {
    3600
}

fn default_search_provider() -> String {
    "exa".to_string()
}

const fn default_quality() -> Quality {
    Quality::High
}

const fn default_target_companies() -> u32 {
    1
}

// ─── Enums ───────────────────────────────────────────────────────────────────

/// Controls how the bot processes room topics each run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// Process only topics not yet covered (incremental).
    Iterate,
    /// Re-process all topics from scratch.
    Full,
    /// Back-fill historical data without generating new polls.
    Backfill,
}

/// Research depth / cost trade-off for a bot run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    Medium,
    High,
}

// ─── BotConfig ────────────────────────────────────────────────────────────────

/// Bot configuration extracted from a room's `engine_config` JSONB.
#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    /// Whether the bot is enabled for this room.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// How the bot iterates over topics each run.
    #[serde(default = "default_run_mode")]
    pub run_mode: RunMode,

    /// Seconds between scheduled bot runs.
    #[serde(default = "default_schedule_secs")]
    pub schedule_secs: u64,

    /// LLM model override (uses platform default when `None`).
    pub model: Option<String>,

    /// Search provider identifier (e.g. `"exa"`).
    #[serde(default = "default_search_provider")]
    pub search_provider: String,

    /// Research quality level.
    #[serde(default = "default_quality")]
    pub quality: Quality,

    /// Number of companies to target per run.
    #[serde(default = "default_target_companies")]
    pub target_companies: u32,
}

impl BotConfig {
    /// Extract bot config from a room's `engine_config` JSONB.
    ///
    /// Returns `None` if the `"bot"` key is missing or `enabled` is `false`.
    #[must_use]
    pub fn from_engine_config(config: &serde_json::Value) -> Option<Self> {
        let bot_value = config.get("bot")?;
        let bot: Self = serde_json::from_value(bot_value.clone()).ok()?;
        if bot.enabled {
            Some(bot)
        } else {
            None
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_from_engine_config_enabled() {
        let config = json!({
            "bot": {
                "enabled": true,
                "run_mode": "full",
                "schedule_secs": 7200,
                "model": "claude-3-5-sonnet",
                "search_provider": "brave",
                "quality": "medium",
                "target_companies": 5
            }
        });

        let bot = BotConfig::from_engine_config(&config).expect("should parse enabled bot config");
        assert!(bot.enabled);
        assert_eq!(bot.run_mode, RunMode::Full);
        assert_eq!(bot.schedule_secs, 7200);
        assert_eq!(bot.model.as_deref(), Some("claude-3-5-sonnet"));
        assert_eq!(bot.search_provider, "brave");
        assert_eq!(bot.quality, Quality::Medium);
        assert_eq!(bot.target_companies, 5);
    }

    #[test]
    fn test_from_engine_config_disabled() {
        let config = json!({
            "bot": {
                "enabled": false,
                "run_mode": "iterate"
            }
        });

        let result = BotConfig::from_engine_config(&config);
        assert!(result.is_none(), "disabled bot should return None");
    }

    #[test]
    fn test_from_engine_config_missing() {
        let config = json!({ "some_other_key": 42 });

        let result = BotConfig::from_engine_config(&config);
        assert!(result.is_none(), "missing bot key should return None");
    }

    #[test]
    fn test_defaults() {
        // Minimal JSON: only `enabled: true`, everything else uses defaults.
        let config = json!({
            "bot": { "enabled": true }
        });

        let bot = BotConfig::from_engine_config(&config).expect("should parse with defaults");
        assert_eq!(bot.run_mode, RunMode::Iterate);
        assert_eq!(bot.schedule_secs, 3600);
        assert!(bot.model.is_none());
        assert_eq!(bot.search_provider, "exa");
        assert_eq!(bot.quality, Quality::High);
        assert_eq!(bot.target_companies, 1);
    }
}
