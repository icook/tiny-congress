//! Service layer for rooms and polling operations
//!
//! Orchestrates vote submission with eligibility checking via the endorsement
//! service and delegates persistence to the rooms repo.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use super::repo::{
    DimensionDistribution, DimensionRecord, DimensionStats, PollRecord, PollRepoError, RoomRecord,
    RoomRepoError, RoomsRepo, VoteRecord,
};
use crate::reputation::service::EndorsementService;

// ─── Request types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    pub votes: Vec<DimensionVote>,
}

#[derive(Debug, Deserialize)]
pub struct DimensionVote {
    pub dimension_id: Uuid,
    pub value: f32,
}

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

#[derive(Debug, thiserror::Error)]
pub enum PollError {
    #[error("{0}")]
    Validation(String),
    #[error("poll not found")]
    PollNotFound,
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, thiserror::Error)]
pub enum VoteError {
    #[error("{0}")]
    Validation(String),
    #[error("not eligible: {0}")]
    NotEligible(String),
    #[error("poll not found")]
    PollNotFound,
    #[error("poll is not active")]
    PollNotActive,
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
    ) -> Result<RoomRecord, RoomError>;
    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomError>;
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomError>;

    // Poll operations
    async fn create_poll(
        &self,
        room_id: Uuid,
        question: &str,
        description: Option<&str>,
    ) -> Result<PollRecord, PollError>;
    async fn list_polls(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollError>;
    async fn get_poll(&self, poll_id: Uuid) -> Result<PollRecord, PollError>;
    async fn activate_poll(&self, poll_id: Uuid) -> Result<(), PollError>;
    async fn close_poll(&self, poll_id: Uuid) -> Result<(), PollError>;

    // Dimension operations
    #[allow(clippy::too_many_arguments)]
    async fn add_dimension(
        &self,
        poll_id: Uuid,
        name: &str,
        description: Option<&str>,
        min_value: f32,
        max_value: f32,
        sort_order: i32,
        min_label: Option<&str>,
        max_label: Option<&str>,
    ) -> Result<DimensionRecord, PollError>;
    async fn list_dimensions(&self, poll_id: Uuid) -> Result<Vec<DimensionRecord>, PollError>;

    // Vote operations (with eligibility check)
    async fn cast_vote(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
        votes: &[DimensionVote],
    ) -> Result<Vec<VoteRecord>, VoteError>;

    // Results
    async fn get_poll_results(&self, poll_id: Uuid) -> Result<PollResults, PollError>;
    async fn get_poll_distribution(&self, poll_id: Uuid) -> Result<PollDistribution, PollError>;
    async fn get_user_votes(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<VoteRecord>, PollError>;
}

#[derive(Debug, Clone)]
pub struct PollResults {
    pub poll: PollRecord,
    pub dimensions: Vec<DimensionStats>,
    pub voter_count: i64,
}

#[derive(Debug, Clone)]
pub struct PollDistribution {
    pub dimensions: Vec<DimensionDistribution>,
}

// ─── Implementation ────────────────────────────────────────────────────────

pub struct DefaultRoomsService {
    repo: Arc<dyn RoomsRepo>,
    endorsement_service: Arc<dyn EndorsementService>,
}

impl DefaultRoomsService {
    #[must_use]
    pub fn new(repo: Arc<dyn RoomsRepo>, endorsement_service: Arc<dyn EndorsementService>) -> Self {
        Self {
            repo,
            endorsement_service,
        }
    }
}

#[async_trait]
impl RoomsService for DefaultRoomsService {
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
    ) -> Result<RoomRecord, RoomError> {
        if name.trim().is_empty() {
            return Err(RoomError::Validation(
                "Room name cannot be empty".to_string(),
            ));
        }
        self.repo
            .create_room(name.trim(), description, eligibility_topic)
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

    async fn create_poll(
        &self,
        room_id: Uuid,
        question: &str,
        description: Option<&str>,
    ) -> Result<PollRecord, PollError> {
        if question.trim().is_empty() {
            return Err(PollError::Validation(
                "Question cannot be empty".to_string(),
            ));
        }
        self.repo
            .create_poll(room_id, question.trim(), description)
            .await
            .map_err(|e| {
                tracing::error!("Poll creation failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn list_polls(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollError> {
        self.repo.list_polls_by_room(room_id).await.map_err(|e| {
            tracing::error!("Poll list failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })
    }

    async fn get_poll(&self, poll_id: Uuid) -> Result<PollRecord, PollError> {
        self.repo.get_poll(poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })
    }

    async fn activate_poll(&self, poll_id: Uuid) -> Result<(), PollError> {
        self.repo
            .update_poll_status(poll_id, "active")
            .await
            .map_err(|e| {
                if matches!(e, PollRepoError::NotFound) {
                    PollError::PollNotFound
                } else {
                    tracing::error!("Poll activation failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn close_poll(&self, poll_id: Uuid) -> Result<(), PollError> {
        self.repo
            .update_poll_status(poll_id, "closed")
            .await
            .map_err(|e| {
                if matches!(e, PollRepoError::NotFound) {
                    PollError::PollNotFound
                } else {
                    tracing::error!("Poll close failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn add_dimension(
        &self,
        poll_id: Uuid,
        name: &str,
        description: Option<&str>,
        min_value: f32,
        max_value: f32,
        sort_order: i32,
        min_label: Option<&str>,
        max_label: Option<&str>,
    ) -> Result<DimensionRecord, PollError> {
        if name.trim().is_empty() {
            return Err(PollError::Validation(
                "Dimension name cannot be empty".to_string(),
            ));
        }
        if min_value >= max_value {
            return Err(PollError::Validation(
                "min_value must be less than max_value".to_string(),
            ));
        }
        self.repo
            .create_dimension(
                poll_id,
                name.trim(),
                description,
                min_value,
                max_value,
                sort_order,
                min_label,
                max_label,
            )
            .await
            .map_err(|e| {
                if matches!(e, PollRepoError::DuplicateDimension) {
                    PollError::Validation("Dimension name already exists for this poll".to_string())
                } else {
                    tracing::error!("Dimension creation failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn list_dimensions(&self, poll_id: Uuid) -> Result<Vec<DimensionRecord>, PollError> {
        self.repo.list_dimensions(poll_id).await.map_err(|e| {
            tracing::error!("Dimension list failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })
    }

    async fn cast_vote(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
        votes: &[DimensionVote],
    ) -> Result<Vec<VoteRecord>, VoteError> {
        // Check poll is active
        let poll = self.repo.get_poll(poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                VoteError::PollNotFound
            } else {
                tracing::error!("Poll lookup for vote failed: {e}");
                VoteError::Internal("Internal server error".to_string())
            }
        })?;

        if poll.status != "active" {
            return Err(VoteError::PollNotActive);
        }

        // Get room to check eligibility topic
        let room = self.repo.get_room(poll.room_id).await.map_err(|e| {
            tracing::error!("Room lookup for vote failed: {e}");
            VoteError::Internal("Internal server error".to_string())
        })?;

        // Eligibility check via endorsement service
        let eligible = self
            .endorsement_service
            .has_endorsement(user_id, &room.eligibility_topic)
            .await
            .map_err(|e| {
                tracing::error!("Eligibility check failed: {e}");
                VoteError::Internal("Internal server error".to_string())
            })?;

        if !eligible {
            return Err(VoteError::NotEligible(format!(
                "You must be verified to vote. Complete identity verification first. Required: {}",
                room.eligibility_topic,
            )));
        }

        // Validate votes against dimensions
        let dimensions = self.repo.list_dimensions(poll_id).await.map_err(|e| {
            tracing::error!("Dimension list for vote failed: {e}");
            VoteError::Internal("Internal server error".to_string())
        })?;

        let dim_map: std::collections::HashMap<Uuid, &super::repo::DimensionRecord> =
            dimensions.iter().map(|d| (d.id, d)).collect();

        if votes.is_empty() {
            return Err(VoteError::Validation(
                "At least one vote is required".to_string(),
            ));
        }

        for v in votes {
            let Some(dim) = dim_map.get(&v.dimension_id) else {
                return Err(VoteError::Validation(format!(
                    "Unknown dimension: {}",
                    v.dimension_id
                )));
            };
            if v.value < dim.min_value || v.value > dim.max_value {
                return Err(VoteError::Validation(format!(
                    "Value {} for dimension '{}' is outside range [{}, {}]",
                    v.value, dim.name, dim.min_value, dim.max_value,
                )));
            }
        }

        // Upsert all votes atomically
        let vote_tuples: Vec<(Uuid, f32)> =
            votes.iter().map(|v| (v.dimension_id, v.value)).collect();
        self.repo
            .upsert_votes_batch(poll_id, user_id, &vote_tuples)
            .await
            .map_err(|e| {
                tracing::error!("Vote upsert failed: {e}");
                VoteError::Internal("Internal server error".to_string())
            })
    }

    async fn get_poll_results(&self, poll_id: Uuid) -> Result<PollResults, PollError> {
        let poll = self.repo.get_poll(poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup for results failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })?;

        let dimensions = self.repo.compute_poll_stats(poll_id).await.map_err(|e| {
            tracing::error!("Poll stats computation failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

        let voter_count = self.repo.count_voters(poll_id).await.map_err(|e| {
            tracing::error!("Voter count failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

        Ok(PollResults {
            poll,
            dimensions,
            voter_count,
        })
    }

    async fn get_poll_distribution(&self, poll_id: Uuid) -> Result<PollDistribution, PollError> {
        // Verify poll exists first
        self.repo.get_poll(poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup for distribution failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })?;

        let dimensions = self
            .repo
            .compute_poll_distribution(poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Poll distribution computation failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        Ok(PollDistribution { dimensions })
    }

    async fn get_user_votes(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<VoteRecord>, PollError> {
        self.repo
            .get_user_votes(poll_id, user_id)
            .await
            .map_err(|e| {
                tracing::error!("User votes lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }
}
