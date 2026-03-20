//! Batch worker — processes trust action queue and recomputes scores.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::reputation::repo::EndorsementRepoError;
use crate::trust::engine::TrustEngine;
use crate::trust::engine::TrustEngineError;
use crate::trust::repo::{ActionRecord, TrustRepo, TrustRepoError};
use crate::trust::service::DENOUNCEMENT_REASON_MAX_LEN;

/// Errors that can occur while processing the trust action queue.
#[derive(Debug, thiserror::Error)]
pub enum TrustWorkerError {
    /// Claiming the next batch of pending actions failed.
    #[error("claim_pending_actions failed: {0}")]
    ClaimActions(#[from] TrustRepoError),
}

/// Errors that can occur while processing a single trust action.
#[derive(Debug, thiserror::Error)]
pub enum TrustActionError {
    /// The action payload is missing a required field or contains an invalid value.
    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    /// A reputation repository operation failed.
    #[error("reputation repo error: {0}")]
    ReputationRepo(#[from] EndorsementRepoError),

    /// A trust repository operation failed.
    #[error("trust repo error: {0}")]
    TrustRepo(#[from] TrustRepoError),

    /// Trust score recomputation failed.
    #[error("engine error: {0}")]
    Engine(#[from] TrustEngineError),

    /// The action type is not recognised.
    #[error("unknown action type: {0}")]
    UnknownActionType(String),
}

use crate::reputation::repo::ReputationRepo;

/// Background worker that claims and processes trust action queue batches.
pub struct TrustWorker {
    trust_repo: Arc<dyn TrustRepo>,
    reputation_repo: Arc<dyn ReputationRepo>,
    trust_engine: Arc<TrustEngine>,
    batch_size: i64,
    batch_interval_secs: u64,
}

impl TrustWorker {
    /// Create a new `TrustWorker`.
    #[must_use]
    pub fn new(
        trust_repo: Arc<dyn TrustRepo>,
        reputation_repo: Arc<dyn ReputationRepo>,
        trust_engine: Arc<TrustEngine>,
        batch_size: i64,
        batch_interval_secs: u64,
    ) -> Self {
        Self {
            trust_repo,
            reputation_repo,
            trust_engine,
            batch_size,
            batch_interval_secs,
        }
    }

    /// Claim and process a batch of pending actions.
    ///
    /// Each action is processed individually; per-action errors are logged and
    /// recorded as `failed` in the queue without aborting the rest of the batch.
    ///
    /// Returns the count of actions processed (regardless of success/failure).
    ///
    /// # Errors
    ///
    /// Returns an error only if claiming actions from the queue fails.
    pub async fn process_batch(&self) -> Result<usize, TrustWorkerError> {
        let actions = self
            .trust_repo
            .claim_pending_actions(self.batch_size)
            .await?;

        let count = actions.len();

        for action in &actions {
            match self.process_action(action).await {
                Ok(()) => {
                    if let Err(e) = self.trust_repo.complete_action(action.id).await {
                        tracing::error!(
                            action_id = %action.id,
                            "failed to mark action complete: {e}"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        action_id = %action.id,
                        action_type = %action.action_type,
                        "action processing error: {e}"
                    );
                    if let Err(fe) = self.trust_repo.fail_action(action.id, &e.to_string()).await {
                        tracing::error!(
                            action_id = %action.id,
                            "failed to mark action failed: {fe}"
                        );
                    }
                }
            }
        }

        Ok(count)
    }

    async fn process_action(&self, action: &ActionRecord) -> Result<(), TrustActionError> {
        match action.action_type.as_str() {
            "endorse" => {
                let subject_id = parse_uuid(&action.payload, "subject_id")?;
                #[allow(clippy::cast_possible_truncation)]
                let weight = action.payload["weight"].as_f64().ok_or_else(|| {
                    TrustActionError::InvalidPayload("endorse payload missing 'weight'".to_string())
                })? as f32;
                if !weight.is_finite() || weight <= 0.0 || weight > 1.0 {
                    return Err(TrustActionError::InvalidPayload(format!(
                        "endorse payload 'weight' out of range (0.0, 1.0]: {weight}"
                    )));
                }
                let attestation = match &action.payload["attestation"] {
                    serde_json::Value::Null => None,
                    v => Some(v.clone()),
                };
                let in_slot = action.payload["in_slot"].as_bool().ok_or_else(|| {
                    TrustActionError::InvalidPayload(
                        "endorse payload missing or invalid 'in_slot'".to_string(),
                    )
                })?;

                self.reputation_repo
                    .create_endorsement(
                        subject_id,
                        "trust",
                        Some(action.actor_id),
                        None,
                        weight,
                        attestation.as_ref(),
                        in_slot,
                    )
                    .await?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            "revoke" => {
                let subject_id = parse_uuid(&action.payload, "subject_id")?;
                self.reputation_repo
                    .revoke_endorsement(action.actor_id, subject_id, "trust")
                    .await?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            "denounce" => {
                let target_id = parse_uuid(&action.payload, "target_id")?;
                let reason = action.payload["reason"]
                    .as_str()
                    .ok_or_else(|| {
                        TrustActionError::InvalidPayload(
                            "denounce payload missing 'reason'".to_string(),
                        )
                    })?
                    .to_string();
                if reason.is_empty() || reason.len() > DENOUNCEMENT_REASON_MAX_LEN {
                    return Err(TrustActionError::InvalidPayload(format!(
                        "denounce payload 'reason' length out of range [1, {}]: {}",
                        DENOUNCEMENT_REASON_MAX_LEN,
                        reason.len()
                    )));
                }

                // Both operations run inside a single transaction: if the endorsement
                // revocation fails after the denouncement is inserted, the whole thing
                // rolls back, preventing the unique-constraint error on retry.
                self.trust_repo
                    .create_denouncement_and_revoke_endorsement(action.actor_id, target_id, &reason)
                    .await?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            other => {
                return Err(TrustActionError::UnknownActionType(other.to_string()));
            }
        }

        Ok(())
    }

    /// Run the worker loop indefinitely, processing a batch immediately on startup
    /// then sleeping for `batch_interval_secs` between runs.
    pub async fn run(self: Arc<Self>) {
        loop {
            if let Err(e) = self.process_batch().await {
                tracing::error!("trust worker batch error: {e}");
            }
            tokio::time::sleep(Duration::from_secs(self.batch_interval_secs)).await;
        }
    }
}

fn parse_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, TrustActionError> {
    let raw = payload[key]
        .as_str()
        .ok_or_else(|| TrustActionError::InvalidPayload(format!("payload missing '{key}'")))?;
    raw.parse::<Uuid>().map_err(|e| {
        TrustActionError::InvalidPayload(format!("payload '{key}' is not a valid UUID: {e}"))
    })
}
