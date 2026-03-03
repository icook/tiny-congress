//! Service layer for reputation operations
//!
//! Provides the [`EndorsementService`] trait that orchestrates endorsement
//! creation, eligibility checks, and verifier bootstrap.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::repo::{
    CreatedEndorsement, EndorsementRecord, EndorsementRepoError, ReputationRepo,
    VerifierAccountRepoError,
};

// ─── Domain error type ─────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum EndorsementError {
    #[error("{0}")]
    Validation(String),
    #[error("endorsement already exists for this subject and topic")]
    Duplicate,
    #[error("verifier not found: {0}")]
    VerifierNotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

// ─── Service trait ─────────────────────────────────────────────────────────

#[async_trait]
pub trait EndorsementService: Send + Sync {
    /// Create an endorsement for a subject on a topic, issued by a verifier.
    async fn create_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
        verifier_name: &str,
        evidence: Option<&serde_json::Value>,
    ) -> Result<CreatedEndorsement, EndorsementError>;

    /// Check if a subject has an active (non-revoked) endorsement for a topic.
    async fn has_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<bool, EndorsementError>;

    /// List all endorsements for a subject.
    async fn list_endorsements(
        &self,
        subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementError>;
}

// ─── Implementation ────────────────────────────────────────────────────────

pub struct DefaultEndorsementService {
    repo: Arc<dyn ReputationRepo>,
}

impl DefaultEndorsementService {
    #[must_use]
    pub fn new(repo: Arc<dyn ReputationRepo>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl EndorsementService for DefaultEndorsementService {
    async fn create_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
        verifier_name: &str,
        evidence: Option<&serde_json::Value>,
    ) -> Result<CreatedEndorsement, EndorsementError> {
        if topic.is_empty() {
            return Err(EndorsementError::Validation(
                "Topic cannot be empty".to_string(),
            ));
        }

        // Look up the verifier by name
        let verifier = self
            .repo
            .get_verifier_account_by_name(verifier_name)
            .await
            .map_err(|e| match e {
                VerifierAccountRepoError::NotFound => {
                    EndorsementError::VerifierNotFound(verifier_name.to_string())
                }
                VerifierAccountRepoError::DuplicateName => {
                    tracing::error!("Unexpected DuplicateName during verifier lookup");
                    EndorsementError::Internal("Internal server error".to_string())
                }
                VerifierAccountRepoError::Database(e) => {
                    tracing::error!("Verifier lookup failed: {e}");
                    EndorsementError::Internal("Internal server error".to_string())
                }
            })?;

        // Create the endorsement
        self.repo
            .create_endorsement(subject_id, topic, verifier.id, evidence)
            .await
            .map_err(|e| match e {
                EndorsementRepoError::Duplicate => EndorsementError::Duplicate,
                EndorsementRepoError::NotFound => {
                    tracing::error!("Unexpected NotFound during endorsement creation");
                    EndorsementError::Internal("Internal server error".to_string())
                }
                EndorsementRepoError::Database(e) => {
                    tracing::error!("Endorsement creation failed: {e}");
                    EndorsementError::Internal("Internal server error".to_string())
                }
            })
    }

    async fn has_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<bool, EndorsementError> {
        self.repo
            .has_endorsement(subject_id, topic)
            .await
            .map_err(|e| match e {
                EndorsementRepoError::Database(e) => {
                    tracing::error!("Endorsement check failed: {e}");
                    EndorsementError::Internal("Internal server error".to_string())
                }
                _ => EndorsementError::Internal("Internal server error".to_string()),
            })
    }

    async fn list_endorsements(
        &self,
        subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementError> {
        self.repo
            .list_endorsements_by_subject(subject_id)
            .await
            .map_err(|e| match e {
                EndorsementRepoError::Database(e) => {
                    tracing::error!("Endorsement list failed: {e}");
                    EndorsementError::Internal("Internal server error".to_string())
                }
                _ => EndorsementError::Internal("Internal server error".to_string()),
            })
    }
}

/// Bootstrap the ID.me verifier account at startup.
/// Returns the verifier account ID.
///
/// # Errors
///
/// Returns `VerifierAccountRepoError` if the database operation fails.
pub async fn bootstrap_idme_verifier(
    repo: &dyn ReputationRepo,
) -> Result<Uuid, VerifierAccountRepoError> {
    let verifier = repo
        .ensure_verifier_account("idme", Some("ID.me identity verification service"))
        .await?;
    tracing::info!(verifier_id = %verifier.id, name = %verifier.name, "ID.me verifier account ready");
    Ok(verifier.id)
}
