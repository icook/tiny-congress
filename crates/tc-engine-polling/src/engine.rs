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

        let llm_api_key = std::env::var("BOT_LLM_API_KEY").unwrap_or_default();
        let exa_api_key = std::env::var("BOT_EXA_API_KEY").unwrap_or_default();
        if llm_api_key.is_empty() || exa_api_key.is_empty() {
            tracing::warn!(
                llm_key_set = !llm_api_key.is_empty(),
                exa_key_set = !exa_api_key.is_empty(),
                "bot worker starting with missing API keys — tasks will fail until configured"
            );
        }

        let bot_config = crate::bot::worker::BotWorkerConfig {
            llm_api_key,
            llm_base_url: std::env::var("BOT_LLM_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:4001".to_string()),
            exa_api_key,
            exa_base_url: std::env::var("BOT_EXA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:4002".to_string()),
            default_model: std::env::var("BOT_DEFAULT_MODEL")
                .unwrap_or_else(|_| "deepseek/deepseek-chat-v3-0324".to_string()),
        };

        let scheduler_handle = crate::bot::scheduler::spawn_scheduler(ctx.pool.clone());

        let bot_handle = crate::bot::worker::spawn_bot_worker(ctx.pool, bot_config);

        Ok(vec![lifecycle_handle, bot_handle, scheduler_handle])
    }
}
