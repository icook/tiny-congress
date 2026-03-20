//! Batch worker — processes trust action queue and recomputes scores.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::reputation::repo::ReputationRepo;
use crate::trust::engine::TrustEngine;
use crate::trust::repo::{ActionRecord, TrustRepo};

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
    pub async fn process_batch(&self) -> Result<usize, anyhow::Error> {
        let actions = self
            .trust_repo
            .claim_pending_actions(self.batch_size)
            .await
            .map_err(|e| anyhow::anyhow!("claim_pending_actions failed: {e}"))?;

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

    async fn process_action(&self, action: &ActionRecord) -> Result<(), anyhow::Error> {
        match action.action_type.as_str() {
            "endorse" => {
                let subject_id = parse_uuid(&action.payload, "subject_id")?;
                #[allow(clippy::cast_possible_truncation)]
                let weight = action.payload["weight"]
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("endorse payload missing 'weight'"))?
                    as f32;
                if !weight.is_finite() || weight <= 0.0 || weight > 1.0 {
                    return Err(anyhow::anyhow!(
                        "endorse payload 'weight' out of range (0.0, 1.0]: {weight}"
                    ));
                }
                let attestation = match &action.payload["attestation"] {
                    serde_json::Value::Null => None,
                    v => Some(v.clone()),
                };
                let in_slot = action.payload["in_slot"].as_bool().ok_or_else(|| {
                    anyhow::anyhow!("endorse payload missing or invalid 'in_slot'")
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
                    .await
                    .map_err(|e| anyhow::anyhow!("create_endorsement failed: {e}"))?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            "revoke" => {
                let subject_id = parse_uuid(&action.payload, "subject_id")?;
                self.reputation_repo
                    .revoke_endorsement(action.actor_id, subject_id, "trust")
                    .await
                    .map_err(|e| anyhow::anyhow!("revoke_endorsement failed: {e}"))?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            "denounce" => {
                let target_id = parse_uuid(&action.payload, "target_id")?;
                let reason = action.payload["reason"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("denounce payload missing 'reason'"))?
                    .to_string();
                if reason.is_empty() || reason.len() > 500 {
                    return Err(anyhow::anyhow!(
                        "denounce payload 'reason' length out of range [1, 500]: {}",
                        reason.len()
                    ));
                }

                // Both operations run inside a single transaction: if the endorsement
                // revocation fails after the denouncement is inserted, the whole thing
                // rolls back, preventing the unique-constraint error on retry.
                self.trust_repo
                    .create_denouncement_and_revoke_endorsement(action.actor_id, target_id, &reason)
                    .await
                    .map_err(|e| anyhow::anyhow!("denounce_and_revoke failed: {e}"))?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            other => {
                return Err(anyhow::anyhow!("unknown action type: {other}"));
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

fn parse_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, anyhow::Error> {
    let raw = payload[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("payload missing '{key}'"))?;
    raw.parse::<Uuid>()
        .map_err(|e| anyhow::anyhow!("payload '{key}' is not a valid UUID: {e}"))
}
