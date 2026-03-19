//! Polling engine — implements [`tc_engine_api::RoomEngine`] for poll-type rooms.

use std::sync::Arc;
use std::time::Duration;

use tc_engine_api::engine::{EngineContext, EngineMetadata, PlatformState};
use tc_engine_api::error::EngineError;
use tc_engine_api::RoomEngine;
use uuid::Uuid;

use crate::lifecycle::spawn_lifecycle_consumer;
use crate::service::DefaultPollingService;

/// Default lifecycle consumer poll interval (seconds).
const LIFECYCLE_POLL_INTERVAL_SECS: u64 = 5;

/// Polling engine plugin.
///
/// Owns the lifecycle consumer and (eventually) the poll HTTP routes.
/// Registered in the [`tc_engine_api::EngineRegistry`] at startup.
pub struct PollingEngine;

impl PollingEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for PollingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RoomEngine for PollingEngine {
    fn engine_type(&self) -> &'static str {
        "polling"
    }

    fn metadata(&self) -> EngineMetadata {
        EngineMetadata {
            display_name: "Polling".to_string(),
            description: "Multi-dimensional polling with eligibility constraints".to_string(),
        }
    }

    fn routes(&self) -> axum::Router<PlatformState> {
        // Poll HTTP handlers still live in the service crate due to the
        // AuthenticatedDevice circular-dependency constraint. They will move
        // here once the auth extractor is extracted to a shared crate.
        axum::Router::new()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn validate_config(&self, _config: &serde_json::Value) -> Result<(), EngineError> {
        Ok(())
    }

    async fn on_room_created(
        &self,
        _room_id: Uuid,
        _config: &serde_json::Value,
        _ctx: &EngineContext,
    ) -> Result<(), EngineError> {
        // No per-room setup needed yet — polls are created via the HTTP API.
        Ok(())
    }

    fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error> {
        let polling_service = Arc::new(DefaultPollingService::new(
            ctx.pool.clone(),
            ctx.trust_reader.clone(),
        ));

        let lifecycle_handle = spawn_lifecycle_consumer(
            ctx.pool.clone(),
            polling_service,
            Duration::from_secs(LIFECYCLE_POLL_INTERVAL_SECS),
        );

        let bot_handle = crate::bot::worker::spawn_bot_worker(ctx.pool);

        Ok(vec![lifecycle_handle, bot_handle])
    }
}
