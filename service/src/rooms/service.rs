//! Service layer for room CRUD operations
//!
//! Poll, vote, dimension, lifecycle, and results operations have moved to
//! [`tc_engine_polling::service::PollingService`].

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::repo::{RoomRecord, RoomRepoError, RoomsRepo};

// Re-export polling types for backward compatibility (used by HTTP handlers & tests)
pub use tc_engine_polling::service::{
    CastVoteRequest, CreateEvidenceItem, DimensionVote, PollDistribution, PollError, PollResults,
    PollingService, VoteError,
};

// ─── Error types ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    #[error("{0}")]
    Validation(String),
    #[error("room not found")]
    RoomNotFound,
    #[error("room name already exists")]
    DuplicateRoomName,
    #[error("internal error: {0}")]
    Internal(String),
}

// ─── Service trait ─────────────────────────────────────────────────────────

#[async_trait]
pub trait RoomsService: Send + Sync {
    // Room operations
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
        poll_duration_secs: Option<i32>,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
    ) -> Result<RoomRecord, RoomError>;
    async fn rooms_needing_content(&self) -> Result<Vec<RoomRecord>, RoomError>;
    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomError>;
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomError>;
}

// ─── Implementation ────────────────────────────────────────────────────────

pub struct DefaultRoomsService {
    repo: Arc<dyn RoomsRepo>,
}

impl DefaultRoomsService {
    #[must_use]
    pub fn new(repo: Arc<dyn RoomsRepo>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl RoomsService for DefaultRoomsService {
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
        poll_duration_secs: Option<i32>,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
    ) -> Result<RoomRecord, RoomError> {
        if name.trim().is_empty() {
            return Err(RoomError::Validation(
                "Room name cannot be empty".to_string(),
            ));
        }
        self.repo
            .create_room(
                name.trim(),
                description,
                eligibility_topic,
                poll_duration_secs,
                constraint_type,
                constraint_config,
            )
            .await
            .map_err(|e| match e {
                RoomRepoError::DuplicateName => RoomError::DuplicateRoomName,
                RoomRepoError::NotFound => RoomError::RoomNotFound,
                RoomRepoError::Database(e) => {
                    tracing::error!("Room creation failed: {e}");
                    RoomError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomError> {
        self.repo.list_rooms(status).await.map_err(|e| {
            tracing::error!("Room list failed: {e}");
            RoomError::Internal("Internal server error".to_string())
        })
    }

    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomError> {
        self.repo.get_room(room_id).await.map_err(|e| {
            if matches!(e, RoomRepoError::NotFound) {
                RoomError::RoomNotFound
            } else {
                tracing::error!("Room lookup failed: {e}");
                RoomError::Internal("Internal server error".to_string())
            }
        })
    }

    async fn rooms_needing_content(&self) -> Result<Vec<RoomRecord>, RoomError> {
        self.repo.rooms_needing_content().await.map_err(|e| {
            tracing::error!("rooms_needing_content failed: {e}");
            RoomError::Internal("Internal server error".to_string())
        })
    }
}
