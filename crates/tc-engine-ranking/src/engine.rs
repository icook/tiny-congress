//! Ranking engine — implements [`tc_engine_api::RoomEngine`] for ranking-type rooms.

use tc_engine_api::engine::{EngineContext, EngineMetadata, PlatformState};
use tc_engine_api::error::EngineError;
use tc_engine_api::RoomEngine;
use uuid::Uuid;

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

    fn validate_config(&self, _config: &serde_json::Value) -> Result<(), EngineError> {
        // Will be implemented in Task 9.
        Ok(())
    }

    async fn on_room_created(
        &self,
        _room_id: Uuid,
        _config: &serde_json::Value,
        _ctx: &EngineContext,
    ) -> Result<(), EngineError> {
        // Will be implemented in a later task.
        Ok(())
    }

    fn start(
        &self,
        _ctx: EngineContext,
    ) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error> {
        // Background tasks will be added in a later task.
        Ok(vec![])
    }
}
