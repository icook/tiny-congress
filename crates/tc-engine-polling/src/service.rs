//! Service layer for polling operations
//!
//! Orchestrates vote submission with eligibility checking via the constraint
//! registry and delegates persistence to the polling repo functions.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use crate::repo::bot_traces::BotTrace;
use crate::repo::{
    bot_traces, evidence, lifecycle_queue, polls, votes, DimensionDistribution, DimensionRecord,
    DimensionStats, EvidenceRecord, PollRecord, PollRepoError, VoteRecord,
};
use tc_engine_api::constraints::build_constraint;
use tc_engine_api::trust::TrustGraphReader;

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

/// Owned evidence item used when creating evidence through the service layer.
#[derive(Debug)]
pub struct CreateEvidenceItem {
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
}

// ─── Error types ───────────────────────────────────────────────────────────

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

// ─── Result types ──────────────────────────────────────────────────────────

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

// ─── Service trait ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait PollingService: Send + Sync {
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

    /// List draft polls in a room's agenda, ordered by position.
    async fn get_agenda(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollError>;

    // Lifecycle operations
    /// Close the active poll and activate the next one from the agenda.
    async fn close_poll_and_advance(&self, room_id: Uuid, poll_id: Uuid) -> Result<(), PollError>;
    /// Activate the next poll from a room's agenda (if any).
    async fn activate_next_from_agenda(&self, room_id: Uuid) -> Result<(), PollError>;

    // Results
    async fn get_poll_results(&self, poll_id: Uuid) -> Result<PollResults, PollError>;
    async fn get_poll_distribution(&self, poll_id: Uuid) -> Result<PollDistribution, PollError>;
    async fn get_user_votes(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<VoteRecord>, PollError>;

    // Evidence operations
    async fn get_evidence_for_dimensions(
        &self,
        dimension_ids: &[Uuid],
    ) -> Result<Vec<EvidenceRecord>, PollError>;
    async fn create_evidence(
        &self,
        poll_id: Uuid,
        dimension_id: Uuid,
        items: Vec<CreateEvidenceItem>,
    ) -> Result<u64, PollError>;
    async fn delete_evidence_for_poll(&self, poll_id: Uuid) -> Result<u64, PollError>;

    /// Sim-only: reset a poll back to draft status, clearing all timing fields.
    async fn reset_poll(&self, room_id: Uuid, poll_id: Uuid) -> Result<(), PollError>;

    // Bot trace operations
    async fn get_poll_traces(&self, poll_id: Uuid) -> Result<Vec<BotTrace>, PollError>;
}

// ─── Implementation ────────────────────────────────────────────────────────

pub struct DefaultPollingService {
    pool: sqlx::PgPool,
    trust_reader: Arc<dyn TrustGraphReader>,
}

impl DefaultPollingService {
    #[must_use]
    pub fn new(pool: sqlx::PgPool, trust_reader: Arc<dyn TrustGraphReader>) -> Self {
        Self { pool, trust_reader }
    }
}

/// Helper to look up a room by ID via direct SQL (the polling service doesn't
/// own a `RoomsRepo`, but needs room metadata for constraint checks and
/// lifecycle cadence).
async fn get_room_record(pool: &sqlx::PgPool, room_id: Uuid) -> Result<RoomRecord, PollError> {
    let row: Option<RoomRecord> = sqlx::query_as(
        r"SELECT poll_duration_secs, constraint_type, constraint_config
          FROM rooms__rooms WHERE id = $1",
    )
    .bind(room_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Room lookup failed: {e}");
        PollError::Internal("Internal server error".to_string())
    })?;

    row.ok_or_else(|| PollError::Internal("Room not found".to_string()))
}

/// Lightweight row type for the room lookup needed by polling service.
#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomRecord {
    poll_duration_secs: Option<i32>,
    constraint_type: String,
    constraint_config: serde_json::Value,
}

#[async_trait]
impl PollingService for DefaultPollingService {
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
        let position = polls::next_agenda_position(&self.pool, room_id)
            .await
            .map_err(|e| {
                tracing::error!("Agenda position lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;
        let poll = polls::create_poll(
            &self.pool,
            room_id,
            question.trim(),
            description,
            Some(position),
        )
        .await
        .map_err(|e| {
            tracing::error!("Poll creation failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

        // Auto-activate if room has cadence and no active poll
        let room = get_room_record(&self.pool, room_id).await?;

        if room.poll_duration_secs.is_some() {
            let active = polls::get_active_poll(&self.pool, room_id)
                .await
                .map_err(|e| {
                    tracing::error!("Active poll check failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                })?;

            if active.is_none() {
                // This is the first poll or agenda was empty — activate it
                polls::update_poll_status(&self.pool, poll.id, "active")
                    .await
                    .map_err(|e| {
                        tracing::error!("Auto-activate failed: {e}");
                        PollError::Internal("Internal server error".to_string())
                    })?;

                if let Some(duration_secs) = room.poll_duration_secs {
                    let closes_at =
                        chrono::Utc::now() + chrono::Duration::seconds(i64::from(duration_secs));
                    polls::set_poll_closes_at(&self.pool, poll.id, closes_at)
                        .await
                        .map_err(|e| {
                            tracing::error!("Set closes_at on auto-activate failed: {e}");
                            PollError::Internal("Internal server error".to_string())
                        })?;

                    lifecycle_queue::enqueue_lifecycle_event(
                        &self.pool,
                        &lifecycle_queue::LifecyclePayload::ClosePoll {
                            poll_id: poll.id,
                            room_id,
                        },
                        f64::from(duration_secs),
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Enqueue close event failed: {e}");
                        PollError::Internal("Internal server error".to_string())
                    })?;
                }

                // Re-fetch to return updated status
                return polls::get_poll(&self.pool, poll.id).await.map_err(|e| {
                    tracing::error!("Poll re-fetch failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                });
            }
        }

        Ok(poll)
    }

    async fn list_polls(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollError> {
        polls::list_polls_by_room(&self.pool, room_id)
            .await
            .map_err(|e| {
                tracing::error!("Poll list failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn get_poll(&self, poll_id: Uuid) -> Result<PollRecord, PollError> {
        polls::get_poll(&self.pool, poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })
    }

    async fn activate_poll(&self, poll_id: Uuid) -> Result<(), PollError> {
        polls::update_poll_status(&self.pool, poll_id, "active")
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
        polls::update_poll_status(&self.pool, poll_id, "closed")
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

    async fn get_agenda(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollError> {
        polls::list_agenda(&self.pool, room_id).await.map_err(|e| {
            tracing::error!("Agenda list failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })
    }

    async fn close_poll_and_advance(&self, room_id: Uuid, poll_id: Uuid) -> Result<(), PollError> {
        // 1. Close the poll (idempotent — if already closed, skip)
        let poll = polls::get_poll(&self.pool, poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })?;

        if poll.status == "active" {
            polls::update_poll_status(&self.pool, poll_id, "closed")
                .await
                .map_err(|e| {
                    tracing::error!("Poll close failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                })?;
            tracing::info!(poll_id = %poll_id, room_id = %room_id, "closed poll");
        }

        // 2. Activate next from agenda
        self.activate_next_from_agenda(room_id).await
    }

    async fn activate_next_from_agenda(&self, room_id: Uuid) -> Result<(), PollError> {
        let next = polls::next_agenda_poll(&self.pool, room_id)
            .await
            .map_err(|e| {
                tracing::error!("Next agenda poll lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        let Some(next_poll) = next else {
            tracing::info!(room_id = %room_id, "agenda empty, room idle");
            return Ok(());
        };

        // Activate the poll
        polls::update_poll_status(&self.pool, next_poll.id, "active")
            .await
            .map_err(|e| {
                tracing::error!("Poll activation failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        // Set closes_at and enqueue close event based on room cadence
        let room = get_room_record(&self.pool, room_id).await?;

        if let Some(duration_secs) = room.poll_duration_secs {
            let closes_at =
                chrono::Utc::now() + chrono::Duration::seconds(i64::from(duration_secs));
            polls::set_poll_closes_at(&self.pool, next_poll.id, closes_at)
                .await
                .map_err(|e| {
                    tracing::error!("Set closes_at failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                })?;

            // Enqueue close event
            lifecycle_queue::enqueue_lifecycle_event(
                &self.pool,
                &lifecycle_queue::LifecyclePayload::ClosePoll {
                    poll_id: next_poll.id,
                    room_id,
                },
                f64::from(duration_secs),
            )
            .await
            .map_err(|e| {
                tracing::error!("Enqueue close event failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;
        }

        tracing::info!(poll_id = %next_poll.id, room_id = %room_id, "activated next poll from agenda");
        Ok(())
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
        polls::create_dimension(
            &self.pool,
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
        polls::list_dimensions(&self.pool, poll_id)
            .await
            .map_err(|e| {
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
        let poll = polls::get_poll(&self.pool, poll_id).await.map_err(|e| {
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

        // Get room to check eligibility constraint
        let room = get_room_record(&self.pool, poll.room_id)
            .await
            .map_err(|e| {
                tracing::error!("Room lookup for vote failed: {e}");
                VoteError::Internal(e.to_string())
            })?;

        // Build the room's constraint from its config and evaluate eligibility
        let constraint =
            build_constraint(&room.constraint_type, &room.constraint_config).map_err(|e| {
                tracing::error!("Failed to build room constraint: {e}");
                VoteError::Internal("Internal server error".to_string())
            })?;

        // Anchor and all other policy is encoded in the constraint config — just call check().
        let eligibility = constraint
            .check(user_id, self.trust_reader.as_ref())
            .await
            .map_err(|e| {
                tracing::error!("Eligibility check failed: {e}");
                VoteError::Internal("Internal server error".to_string())
            })?;

        if !eligibility.is_eligible {
            let reason = eligibility
                .reason
                .unwrap_or_else(|| "not eligible to vote in this room".to_string());
            return Err(VoteError::NotEligible(format!(
                "You must be verified to vote. Complete identity verification first. {reason}",
            )));
        }

        // Validate votes against dimensions
        let dimensions = polls::list_dimensions(&self.pool, poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Dimension list for vote failed: {e}");
                VoteError::Internal("Internal server error".to_string())
            })?;

        let dim_map: std::collections::HashMap<Uuid, &DimensionRecord> =
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
        let mut tx = self.pool.begin().await.map_err(|e| {
            tracing::error!("Vote transaction begin failed: {e}");
            VoteError::Internal("Internal server error".to_string())
        })?;
        let mut results = Vec::with_capacity(votes.len());
        for v in votes {
            let record = votes::upsert_vote(&mut *tx, poll_id, v.dimension_id, user_id, v.value)
                .await
                .map_err(|e| {
                    tracing::error!("Vote upsert failed: {e}");
                    VoteError::Internal("Internal server error".to_string())
                })?;
            results.push(record);
        }
        tx.commit().await.map_err(|e| {
            tracing::error!("Vote transaction commit failed: {e}");
            VoteError::Internal("Internal server error".to_string())
        })?;
        Ok(results)
    }

    async fn get_poll_results(&self, poll_id: Uuid) -> Result<PollResults, PollError> {
        let poll = polls::get_poll(&self.pool, poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup for results failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })?;

        let dimensions = votes::compute_poll_stats(&self.pool, poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Poll stats computation failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        let voter_count = votes::count_voters(&self.pool, poll_id)
            .await
            .map_err(|e| {
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
        polls::get_poll(&self.pool, poll_id).await.map_err(|e| {
            if matches!(e, PollRepoError::NotFound) {
                PollError::PollNotFound
            } else {
                tracing::error!("Poll lookup for distribution failed: {e}");
                PollError::Internal("Internal server error".to_string())
            }
        })?;

        let dimensions = votes::compute_poll_distribution(&self.pool, poll_id)
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
        votes::get_user_votes(&self.pool, poll_id, user_id)
            .await
            .map_err(|e| {
                tracing::error!("User votes lookup failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn get_evidence_for_dimensions(
        &self,
        dimension_ids: &[Uuid],
    ) -> Result<Vec<EvidenceRecord>, PollError> {
        evidence::get_evidence_for_dimensions(&self.pool, dimension_ids)
            .await
            .map_err(|e| {
                tracing::error!("Evidence fetch failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn create_evidence(
        &self,
        poll_id: Uuid,
        dimension_id: Uuid,
        items: Vec<CreateEvidenceItem>,
    ) -> Result<u64, PollError> {
        let belongs = polls::dimension_belongs_to_poll(&self.pool, dimension_id, poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Dimension ownership check failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        if !belongs {
            return Err(PollError::Validation(
                "Dimension not found for this poll".to_string(),
            ));
        }

        let new_evidence: Vec<evidence::NewEvidence<'_>> = items
            .iter()
            .map(|item| evidence::NewEvidence {
                stance: &item.stance,
                claim: &item.claim,
                source: item.source.as_deref(),
            })
            .collect();

        evidence::insert_evidence(&self.pool, dimension_id, &new_evidence)
            .await
            .map_err(|e| {
                tracing::error!("Evidence insert failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn delete_evidence_for_poll(&self, poll_id: Uuid) -> Result<u64, PollError> {
        evidence::delete_evidence_for_poll(&self.pool, poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Evidence delete failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }

    async fn reset_poll(&self, room_id: Uuid, poll_id: Uuid) -> Result<(), PollError> {
        polls::reset_poll(&self.pool, room_id, poll_id)
            .await
            .map_err(|e| {
                if matches!(e, PollRepoError::NotFound) {
                    PollError::PollNotFound
                } else {
                    tracing::error!("Poll reset failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn get_poll_traces(&self, poll_id: Uuid) -> Result<Vec<BotTrace>, PollError> {
        bot_traces::get_traces_for_poll(&self.pool, poll_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch poll traces: {e}");
                PollError::Internal("Internal server error".to_string())
            })
    }
}
