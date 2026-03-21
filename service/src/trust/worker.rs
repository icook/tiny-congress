//! pgmq-backed worker — processes trust action log entries one message at a time.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use tc_engine_polling::repo::pgmq;

use crate::reputation::repo::EndorsementRepoError;
use crate::reputation::repo::ReputationRepo;
use crate::trust::engine::TrustEngine;
use crate::trust::engine::TrustEngineError;
use crate::trust::repo::action_queue::QUEUE_NAME;
use crate::trust::repo::{ActionRecord, TrustRepo, TrustRepoError};
use crate::trust::service::{
    is_valid_endorsement_weight, is_valid_reason, ActionType, DENOUNCEMENT_REASON_MAX_LEN,
};

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

    /// The pgmq queue read failed.
    #[error("queue read failed: {0}")]
    Queue(sqlx::Error),
}

const MAX_RETRIES: i32 = 3;
const VISIBILITY_TIMEOUT_SECS: i32 = 120;
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Background worker that reads trust action messages from pgmq and processes them.
pub struct TrustWorker {
    pool: PgPool,
    trust_repo: Arc<dyn TrustRepo>,
    reputation_repo: Arc<dyn ReputationRepo>,
    trust_engine: Arc<TrustEngine>,
}

impl TrustWorker {
    /// Create a new `TrustWorker`.
    #[must_use]
    pub fn new(
        pool: PgPool,
        trust_repo: Arc<dyn TrustRepo>,
        reputation_repo: Arc<dyn ReputationRepo>,
        trust_engine: Arc<TrustEngine>,
    ) -> Self {
        Self {
            pool,
            trust_repo,
            reputation_repo,
            trust_engine,
        }
    }

    /// Enqueue pgmq messages for any pre-migration `status = 'pending'` rows.
    ///
    /// Run once at startup to recover actions that were inserted before the
    /// pgmq queue existed.
    async fn drain_legacy_pending(&self) {
        let rows: Vec<(Uuid,)> = match sqlx::query_as(
            "SELECT id FROM trust__action_log WHERE status = 'pending' ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("drain_legacy_pending: failed to query pending rows: {e}");
                return;
            }
        };

        for (id,) in &rows {
            let payload = serde_json::json!({ "log_id": id.to_string() });
            if let Err(e) = pgmq::send(&self.pool, QUEUE_NAME, &payload).await {
                tracing::error!(action_id = %id, "drain_legacy_pending: failed to enqueue: {e}");
            }
        }

        if !rows.is_empty() {
            tracing::info!(
                count = rows.len(),
                "drain_legacy_pending: enqueued legacy pending actions"
            );
        }
    }

    /// Read and process one message from the pgmq queue.
    ///
    /// Returns `true` if a message was found and processed (regardless of
    /// success or failure), `false` if the queue was empty.
    ///
    /// This is the single-message unit used by the run loop; exposed for
    /// integration tests so they can drive the worker step-by-step without
    /// spinning up the infinite loop.
    ///
    /// # Errors
    ///
    /// Returns an error only if `pgmq::read` itself fails.
    pub async fn process_one(&self) -> Result<bool, TrustActionError> {
        let msg = match pgmq::read(&self.pool, QUEUE_NAME, VISIBILITY_TIMEOUT_SECS).await {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(false),
            Err(e) => return Err(TrustActionError::Queue(e)),
        };

        let msg_id = msg.msg_id;

        // Poison-message guard
        if msg.read_ct > MAX_RETRIES {
            tracing::warn!(
                msg_id,
                read_ct = msg.read_ct,
                "trust worker: poison message detected, marking failed and archiving"
            );
            if let Some(log_id) = extract_log_id(&msg.message) {
                if let Err(e) = self
                    .trust_repo
                    .fail_action(log_id, "poison message: exceeded max retries")
                    .await
                {
                    tracing::error!(msg_id, "trust worker: fail_action for poison msg: {e}");
                }
            }
            if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg_id).await {
                tracing::error!(msg_id, "trust worker: archive poison msg failed: {e}");
            }
            return Ok(true);
        }

        let Some(log_id) = extract_log_id(&msg.message) else {
            tracing::error!(
                msg_id,
                message = ?msg.message,
                "trust worker: missing or invalid log_id in message"
            );
            if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg_id).await {
                tracing::error!(msg_id, "trust worker: archive bad-payload msg failed: {e}");
            }
            return Ok(true);
        };

        let action = match self.trust_repo.get_action(log_id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(msg_id, %log_id, "trust worker: get_action failed: {e}");
                // Leave in queue so visibility timeout re-exposes it for retry
                return Ok(true);
            }
        };

        match self.process_action(&action).await {
            Ok(()) => {
                if let Err(e) = self.trust_repo.complete_action(action.id).await {
                    tracing::error!(
                        action_id = %action.id,
                        "trust worker: failed to mark action complete: {e}"
                    );
                }
                if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg_id).await {
                    tracing::error!(msg_id, "trust worker: archive after complete failed: {e}");
                }
            }
            Err(e) => {
                tracing::error!(
                    action_id = %action.id,
                    action_type = %action.action_type,
                    "trust worker: action processing error: {e}"
                );
                if let Err(fe) = self.trust_repo.fail_action(action.id, &e.to_string()).await {
                    tracing::error!(
                        action_id = %action.id,
                        "trust worker: failed to mark action failed: {fe}"
                    );
                }
                if let Err(ae) = pgmq::archive(&self.pool, QUEUE_NAME, msg_id).await {
                    tracing::error!(msg_id, "trust worker: archive after fail failed: {ae}");
                }
            }
        }

        Ok(true)
    }

    /// Run the worker loop indefinitely.
    ///
    /// Drains any pre-migration pending rows once on startup, then polls
    /// pgmq for new messages in a tight loop, sleeping `POLL_INTERVAL` when
    /// the queue is empty.
    pub async fn run(self: Arc<Self>) {
        self.drain_legacy_pending().await;

        loop {
            match self.process_one().await {
                Ok(true) => {
                    // message was processed; immediately try for the next one
                }
                Ok(false) => {
                    // queue was empty; back off before polling again
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                Err(e) => {
                    tracing::error!("trust worker: pgmq::read error: {e}");
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
    }

    async fn process_action(&self, action: &ActionRecord) -> Result<(), TrustActionError> {
        let action_type = ActionType::from_str_opt(action.action_type.as_str())
            .ok_or_else(|| TrustActionError::UnknownActionType(action.action_type.clone()))?;
        match action_type {
            ActionType::Endorse => {
                let (subject_id, weight, attestation, in_slot) =
                    parse_endorse_payload(&action.payload)?;

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

            ActionType::Revoke => {
                let subject_id = parse_revoke_payload(&action.payload)?;
                self.reputation_repo
                    .revoke_endorsement(action.actor_id, subject_id, "trust")
                    .await?;

                self.trust_engine
                    .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
                    .await?;
            }

            ActionType::Denounce => {
                let (target_id, reason) = parse_denounce_payload(&action.payload)?;

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
        }

        Ok(())
    }
}

fn extract_log_id(message: &serde_json::Value) -> Option<Uuid> {
    message["log_id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
}

fn parse_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, TrustActionError> {
    let raw = payload[key]
        .as_str()
        .ok_or_else(|| TrustActionError::InvalidPayload(format!("payload missing '{key}'")))?;
    raw.parse::<Uuid>().map_err(|e| {
        TrustActionError::InvalidPayload(format!("payload '{key}' is not a valid UUID: {e}"))
    })
}

/// Extract and validate all fields from a `revoke` action payload.
fn parse_revoke_payload(payload: &serde_json::Value) -> Result<Uuid, TrustActionError> {
    parse_uuid(payload, "subject_id")
}

/// Extract and validate all fields from an `endorse` action payload.
fn parse_endorse_payload(
    payload: &serde_json::Value,
) -> Result<(Uuid, f32, Option<serde_json::Value>, bool), TrustActionError> {
    let subject_id = parse_uuid(payload, "subject_id")?;
    #[allow(clippy::cast_possible_truncation)]
    let weight = payload["weight"].as_f64().ok_or_else(|| {
        TrustActionError::InvalidPayload("endorse payload missing 'weight'".to_string())
    })? as f32;
    if !is_valid_endorsement_weight(weight) {
        return Err(TrustActionError::InvalidPayload(format!(
            "endorse payload 'weight' out of range (0.0, 1.0]: {weight}"
        )));
    }
    let attestation = match &payload["attestation"] {
        serde_json::Value::Null => None,
        v => Some(v.clone()),
    };
    let in_slot = payload["in_slot"].as_bool().ok_or_else(|| {
        TrustActionError::InvalidPayload("endorse payload missing or invalid 'in_slot'".to_string())
    })?;
    Ok((subject_id, weight, attestation, in_slot))
}

/// Extract and validate all fields from a `denounce` action payload.
fn parse_denounce_payload(payload: &serde_json::Value) -> Result<(Uuid, String), TrustActionError> {
    let target_id = parse_uuid(payload, "target_id")?;
    let reason = payload["reason"]
        .as_str()
        .ok_or_else(|| {
            TrustActionError::InvalidPayload("denounce payload missing 'reason'".to_string())
        })?
        .to_string();
    if !is_valid_reason(&reason) {
        return Err(TrustActionError::InvalidPayload(format!(
            "denounce payload 'reason' length out of range [1, {}]: {}",
            DENOUNCEMENT_REASON_MAX_LEN,
            reason.chars().count()
        )));
    }
    Ok((target_id, reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- extract_log_id ---

    #[test]
    fn extract_log_id_returns_uuid_for_valid_string() {
        let id = Uuid::new_v4();
        let msg = json!({ "log_id": id.to_string() });
        assert_eq!(extract_log_id(&msg), Some(id));
    }

    #[test]
    fn extract_log_id_returns_none_for_missing_key() {
        let msg = json!({});
        assert_eq!(extract_log_id(&msg), None);
    }

    #[test]
    fn extract_log_id_returns_none_for_non_string_value() {
        let msg = json!({ "log_id": 12345 });
        assert_eq!(extract_log_id(&msg), None);
    }

    #[test]
    fn extract_log_id_returns_none_for_invalid_uuid_string() {
        let msg = json!({ "log_id": "not-a-uuid" });
        assert_eq!(extract_log_id(&msg), None);
    }

    // --- parse_uuid ---

    #[test]
    fn parse_uuid_returns_uuid_for_valid_string() {
        let id = Uuid::new_v4();
        let payload = json!({ "subject_id": id.to_string() });
        assert_eq!(parse_uuid(&payload, "subject_id").unwrap(), id);
    }

    #[test]
    fn parse_uuid_errors_when_key_is_missing() {
        let payload = json!({});
        let err = parse_uuid(&payload, "subject_id").unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("subject_id")),
            "expected InvalidPayload mentioning the key, got: {err}"
        );
    }

    #[test]
    fn parse_uuid_errors_when_value_is_not_a_string() {
        let payload = json!({ "subject_id": 42 });
        let err = parse_uuid(&payload, "subject_id").unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("subject_id")),
            "expected InvalidPayload mentioning the key, got: {err}"
        );
    }

    #[test]
    fn parse_uuid_errors_when_value_is_invalid_uuid() {
        let payload = json!({ "subject_id": "not-a-uuid" });
        let err = parse_uuid(&payload, "subject_id").unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg)
                if msg.contains("subject_id") && msg.contains("not a valid UUID")),
            "expected InvalidPayload mentioning key and 'not a valid UUID', got: {err}"
        );
    }

    // --- parse_revoke_payload ---

    #[test]
    fn parse_revoke_payload_returns_uuid_for_valid_payload() {
        let subject_id = Uuid::new_v4();
        let payload = json!({ "subject_id": subject_id.to_string() });
        assert_eq!(parse_revoke_payload(&payload).unwrap(), subject_id);
    }

    #[test]
    fn parse_revoke_payload_errors_when_subject_id_is_missing() {
        let payload = json!({});
        let err = parse_revoke_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("subject_id")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_revoke_payload_errors_when_subject_id_is_not_a_string() {
        let payload = json!({ "subject_id": 42 });
        let err = parse_revoke_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("subject_id")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_revoke_payload_errors_when_subject_id_is_invalid_uuid() {
        let payload = json!({ "subject_id": "not-a-uuid" });
        let err = parse_revoke_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg)
                if msg.contains("subject_id") && msg.contains("not a valid UUID")),
            "got: {err}"
        );
    }

    // --- parse_endorse_payload ---

    #[test]
    fn parse_endorse_payload_returns_fields_for_valid_payload() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": 0.8,
            "attestation": null,
            "in_slot": true,
        });
        let (sid, weight, attestation, in_slot) = parse_endorse_payload(&payload).unwrap();
        assert_eq!(sid, subject_id);
        assert!((weight - 0.8).abs() < f32::EPSILON);
        assert!(attestation.is_none());
        assert!(in_slot);
    }

    #[test]
    fn parse_endorse_payload_captures_non_null_attestation() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": 1.0,
            "attestation": { "key": "value" },
            "in_slot": false,
        });
        let (_, _, attestation, in_slot) = parse_endorse_payload(&payload).unwrap();
        assert!(attestation.is_some());
        assert!(!in_slot);
    }

    #[test]
    fn parse_endorse_payload_errors_when_subject_id_is_missing() {
        let payload = json!({
            "weight": 0.5,
            "attestation": null,
            "in_slot": true,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("subject_id")),
            "expected InvalidPayload mentioning 'subject_id', got: {err}"
        );
    }

    #[test]
    fn parse_endorse_payload_errors_when_weight_is_missing() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "attestation": null,
            "in_slot": true,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("missing 'weight'")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_endorse_payload_errors_when_weight_is_not_a_number() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": "high",
            "attestation": null,
            "in_slot": true,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("missing 'weight'")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_endorse_payload_errors_when_weight_is_out_of_range() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": 0.0,
            "attestation": null,
            "in_slot": true,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("out of range")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_endorse_payload_errors_when_weight_exceeds_one() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": 1.1,
            "attestation": null,
            "in_slot": true,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("out of range")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_endorse_payload_errors_when_in_slot_is_missing() {
        let subject_id = Uuid::new_v4();
        let payload = json!({
            "subject_id": subject_id.to_string(),
            "weight": 0.5,
            "attestation": null,
        });
        let err = parse_endorse_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("in_slot")),
            "got: {err}"
        );
    }

    // --- parse_denounce_payload ---

    #[test]
    fn parse_denounce_payload_returns_fields_for_valid_payload() {
        let target_id = Uuid::new_v4();
        let payload = json!({
            "target_id": target_id.to_string(),
            "reason": "harmful conduct",
        });
        let (tid, reason) = parse_denounce_payload(&payload).unwrap();
        assert_eq!(tid, target_id);
        assert_eq!(reason, "harmful conduct");
    }

    #[test]
    fn parse_denounce_payload_errors_when_target_id_is_missing() {
        let payload = json!({ "reason": "harmful conduct" });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("target_id")),
            "expected InvalidPayload mentioning 'target_id', got: {err}"
        );
    }

    #[test]
    fn parse_denounce_payload_errors_when_reason_is_missing() {
        let target_id = Uuid::new_v4();
        let payload = json!({ "target_id": target_id.to_string() });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("missing 'reason'")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_denounce_payload_errors_when_reason_is_not_a_string() {
        let target_id = Uuid::new_v4();
        let payload = json!({ "target_id": target_id.to_string(), "reason": 42 });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("missing 'reason'")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_denounce_payload_errors_when_reason_is_empty() {
        let target_id = Uuid::new_v4();
        let payload = json!({ "target_id": target_id.to_string(), "reason": "" });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("out of range")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_denounce_payload_errors_when_reason_is_whitespace_only() {
        let target_id = Uuid::new_v4();
        let payload = json!({ "target_id": target_id.to_string(), "reason": "   " });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("out of range")),
            "got: {err}"
        );
    }

    #[test]
    fn parse_denounce_payload_errors_when_reason_exceeds_max_len() {
        let target_id = Uuid::new_v4();
        let long_reason = "x".repeat(DENOUNCEMENT_REASON_MAX_LEN + 1);
        let payload = json!({ "target_id": target_id.to_string(), "reason": long_reason });
        let err = parse_denounce_payload(&payload).unwrap_err();
        assert!(
            matches!(err, TrustActionError::InvalidPayload(ref msg) if msg.contains("out of range")),
            "got: {err}"
        );
    }
}
