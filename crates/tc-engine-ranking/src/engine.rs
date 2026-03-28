//! Ranking engine — implements [`tc_engine_api::RoomEngine`] for ranking-type rooms.

use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveTime;
use tc_engine_api::engine::{EngineContext, EngineMetadata, PlatformState};
use tc_engine_api::error::EngineError;
use tc_engine_api::RoomEngine;
use uuid::Uuid;

use crate::config::RankingConfig;
use crate::lifecycle::{
    enqueue_ranking_event, spawn_ranking_lifecycle_consumer, RankingLifecyclePayload,
};
use crate::service::DefaultRankingService;

/// Default lifecycle consumer poll interval (seconds).
const LIFECYCLE_POLL_INTERVAL_SECS: u64 = 5;

/// Ranking engine plugin.
///
/// Owns the pair-selection scheduler and Glicko-2 rating logic.
/// Registered in the [`tc_engine_api::EngineRegistry`] at startup.
pub struct RankingEngine;

impl RankingEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for RankingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RoomEngine for RankingEngine {
    fn engine_type(&self) -> &'static str {
        "ranking"
    }

    fn metadata(&self) -> EngineMetadata {
        EngineMetadata {
            display_name: "Ranking".to_string(),
            description: "Pairwise ranking with Glicko-2 ratings and hall of fame".to_string(),
        }
    }

    fn routes(&self) -> axum::Router<PlatformState> {
        // HTTP handlers will live in the service crate initially.
        axum::Router::new()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn validate_config(&self, config: &serde_json::Value) -> Result<(), EngineError> {
        let config: RankingConfig = serde_json::from_value(config.clone())
            .map_err(|e| EngineError::InvalidInput(format!("invalid ranking config: {e}")))?;

        // Validate anchor_time parses as HH:MM:SS
        NaiveTime::parse_from_str(&config.anchor_time, "%H:%M:%S")
            .map_err(|_| EngineError::InvalidInput("anchor_time must be HH:MM:SS".into()))?;

        // Validate timezone is a valid IANA string
        config
            .anchor_timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| EngineError::InvalidInput("invalid timezone".into()))?;

        // Validate durations are at least 1 hour
        if config.submit_duration_secs < 3600 {
            return Err(EngineError::InvalidInput(
                "submit_duration_secs must be >= 3600 (1 hour)".into(),
            ));
        }
        if config.rank_duration_secs < 3600 {
            return Err(EngineError::InvalidInput(
                "rank_duration_secs must be >= 3600 (1 hour)".into(),
            ));
        }

        // Validate hall_of_fame_depth is in 1–10
        if config.hall_of_fame_depth < 1 || config.hall_of_fame_depth > 10 {
            return Err(EngineError::InvalidInput(
                "hall_of_fame_depth must be between 1 and 10".into(),
            ));
        }

        Ok(())
    }

    async fn on_room_created(
        &self,
        room_id: Uuid,
        config: &serde_json::Value,
        ctx: &EngineContext,
    ) -> Result<(), EngineError> {
        let config: RankingConfig = serde_json::from_value(config.clone())
            .map_err(|e| EngineError::InvalidInput(format!("invalid ranking config: {e}")))?;

        // Calculate delay until the next configured anchor time.
        let delay_secs = config.seconds_until_next_anchor();

        enqueue_ranking_event(
            &ctx.pool,
            RankingLifecyclePayload::OpenSubmit { room_id },
            delay_secs,
        )
        .await
        .map_err(|e| EngineError::Internal(e.into()))?;

        tracing::info!(
            room_id = %room_id,
            delay_secs,
            "ranking room created; OpenSubmit enqueued"
        );

        Ok(())
    }

    fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error> {
        let ranking_service = Arc::new(DefaultRankingService::new(ctx.pool.clone()));

        let handle = spawn_ranking_lifecycle_consumer(
            ctx.pool.clone(),
            ranking_service,
            ctx.room_lifecycle.clone(),
            Duration::from_secs(LIFECYCLE_POLL_INTERVAL_SECS),
        );

        Ok(vec![handle])
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "America/New_York",
            "submit_duration_secs": 86400,
            "rank_duration_secs": 86400,
            "hall_of_fame_depth": 3
        });
        assert!(engine.validate_config(&config).is_ok());
    }

    #[test]
    fn test_defaults_applied() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "12:00:00",
            "anchor_timezone": "UTC"
        });
        assert!(engine.validate_config(&config).is_ok());
    }

    #[test]
    fn test_bad_timezone() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "Not/A/Timezone"
        });
        let err = engine.validate_config(&config).unwrap_err();
        assert!(matches!(err, EngineError::InvalidInput(_)));
    }

    #[test]
    fn test_bad_time_format() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "25:00:00",
            "anchor_timezone": "UTC"
        });
        let err = engine.validate_config(&config).unwrap_err();
        assert!(matches!(err, EngineError::InvalidInput(_)));
    }

    #[test]
    fn test_duration_too_short() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "UTC",
            "submit_duration_secs": 60
        });
        let err = engine.validate_config(&config).unwrap_err();
        assert!(matches!(err, EngineError::InvalidInput(_)));
    }

    #[test]
    fn test_rank_duration_too_short() {
        let engine = RankingEngine;
        let config = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "UTC",
            "submit_duration_secs": 86400,
            "rank_duration_secs": 1800
        });
        let err = engine.validate_config(&config).unwrap_err();
        assert!(matches!(err, EngineError::InvalidInput(_)));
    }

    #[test]
    fn test_hall_of_fame_depth_out_of_range() {
        let engine = RankingEngine;

        let too_low = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "UTC",
            "hall_of_fame_depth": 0
        });
        assert!(engine.validate_config(&too_low).is_err());

        let too_high = serde_json::json!({
            "anchor_time": "18:00:00",
            "anchor_timezone": "UTC",
            "hall_of_fame_depth": 11
        });
        assert!(engine.validate_config(&too_high).is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        let engine = RankingEngine;
        // anchor_time is required
        let config = serde_json::json!({
            "anchor_timezone": "UTC"
        });
        assert!(engine.validate_config(&config).is_err());
    }
}
