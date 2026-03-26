//! Room constraint trait and preset implementations.
//!
//! Constraints determine whether a user may participate in a room. Each
//! constraint is a self-contained policy object: all configuration (including
//! anchor identity for trust-graph constraints) is captured at construction
//! time via [`build_constraint`]. Callers only need `user_id` and a
//! [`TrustGraphReader`].
//!
//! This module is a dependency-light port of the service-layer constraints
//! (`service/src/trust/constraints.rs`), swapping the concrete `TrustRepo`
//! for the abstract [`TrustGraphReader`] trait.

use async_trait::async_trait;
use uuid::Uuid;

use crate::trust::TrustGraphReader;

// ---------------------------------------------------------------------------
// Eligibility
// ---------------------------------------------------------------------------

/// Result of a room eligibility check.
#[derive(Debug)]
pub struct Eligibility {
    pub is_eligible: bool,
    /// Human-readable explanation if ineligible.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// RoomConstraint trait
// ---------------------------------------------------------------------------

/// Pluggable constraint that determines whether a user may participate in a room.
///
/// Constraints are self-contained policy objects: all configuration (including
/// anchor identity for trust-graph constraints) is captured at construction
/// time via [`build_constraint`]. Callers only need `user_id` and the reader.
#[async_trait]
pub trait RoomConstraint: Send + Sync {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error>;
}

// ---------------------------------------------------------------------------
// EndorsedByConstraint
// ---------------------------------------------------------------------------

/// User must appear in the trust graph reachable from the configured anchor.
pub struct EndorsedByConstraint {
    anchor_id: Uuid,
}

impl EndorsedByConstraint {
    /// Create a new constraint requiring the user to be reachable from `anchor_id`.
    #[must_use]
    pub const fn new(anchor_id: Uuid) -> Self {
        Self { anchor_id }
    }
}

#[async_trait]
impl RoomConstraint for EndorsedByConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;

        // Any score at all means the user is reachable from the anchor.
        match snapshot {
            Some(_) => Ok(Eligibility {
                is_eligible: true,
                reason: None,
            }),
            None => Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "not reachable from room anchor {} in trust graph",
                    self.anchor_id
                )),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// CommunityConstraint
// ---------------------------------------------------------------------------

/// User must have `trust_distance <= max_distance` AND `path_diversity >= min_diversity`.
#[derive(Debug)]
pub struct CommunityConstraint {
    anchor_id: Uuid,
    max_distance: f64,
    min_diversity: u32,
}

impl CommunityConstraint {
    /// Create a new `CommunityConstraint`, validating that values are in range.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_distance` is not in `(0.0, 100.0]` or `min_diversity < 1`.
    pub fn new(
        anchor_id: Uuid,
        max_distance: f64,
        min_diversity: u32,
    ) -> Result<Self, anyhow::Error> {
        if max_distance <= 0.0 || max_distance > 100.0 {
            return Err(anyhow::anyhow!(
                "max_distance must be in (0.0, 100.0], got {max_distance}"
            ));
        }
        if min_diversity < 1 {
            return Err(anyhow::anyhow!(
                "min_diversity must be >= 1, got {min_diversity}"
            ));
        }
        Ok(Self {
            anchor_id,
            max_distance,
            min_diversity,
        })
    }
}

#[async_trait]
impl RoomConstraint for CommunityConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;

        let Some(snap) = snapshot else {
            return Ok(Eligibility {
                is_eligible: false,
                reason: Some("no trust score found for user".to_string()),
            });
        };

        let mut failures: Vec<String> = Vec::new();

        if snap.trust_distance > self.max_distance {
            failures.push(format!(
                "trust distance {:.2} exceeds maximum {:.2}",
                snap.trust_distance, self.max_distance
            ));
        }

        if snap.path_diversity < self.min_diversity {
            failures.push(format!(
                "path diversity {} is below minimum {}",
                snap.path_diversity, self.min_diversity
            ));
        }

        if failures.is_empty() {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(failures.join("; ")),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// CongressConstraint
// ---------------------------------------------------------------------------

/// User must have `path_diversity >= min_diversity` (stricter sybil resistance).
#[derive(Debug)]
pub struct CongressConstraint {
    anchor_id: Uuid,
    min_diversity: u32,
}

impl CongressConstraint {
    /// Create a new `CongressConstraint`, validating that `min_diversity >= 1`.
    ///
    /// # Errors
    ///
    /// Returns an error if `min_diversity < 1`.
    pub fn new(anchor_id: Uuid, min_diversity: u32) -> Result<Self, anyhow::Error> {
        if min_diversity < 1 {
            return Err(anyhow::anyhow!(
                "min_diversity must be >= 1, got {min_diversity}"
            ));
        }
        Ok(Self {
            anchor_id,
            min_diversity,
        })
    }
}

#[async_trait]
impl RoomConstraint for CongressConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_reader
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;

        let Some(snap) = snapshot else {
            return Ok(Eligibility {
                is_eligible: false,
                reason: Some("no trust score found for user".to_string()),
            });
        };

        if snap.path_diversity < self.min_diversity {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "path diversity {} is below minimum {}",
                    snap.path_diversity, self.min_diversity
                )),
            })
        } else {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// EndorsedByUserConstraint
// ---------------------------------------------------------------------------

/// User must have a trust endorsement from a specific account (the room owner).
/// Includes out-of-slot endorsements. The endorser (owner) always passes.
pub struct EndorsedByUserConstraint {
    endorser_id: Uuid,
}

impl EndorsedByUserConstraint {
    /// Create a new constraint requiring a direct endorsement from `endorser_id`.
    #[must_use]
    pub const fn new(endorser_id: Uuid) -> Self {
        Self { endorser_id }
    }
}

#[async_trait]
impl RoomConstraint for EndorsedByUserConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        if user_id == self.endorser_id {
            return Ok(Eligibility {
                is_eligible: true,
                reason: None,
            });
        }
        let has = trust_reader
            .has_endorsement(user_id, "trust", &[self.endorser_id])
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;
        if has {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "requires endorsement from room owner {}",
                    self.endorser_id
                )),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// IdentityVerifiedConstraint (Layer 1)
// ---------------------------------------------------------------------------

/// Layer 1 constraint: checks identity attestation from recognised verifiers.
///
/// Queries endorsements directly — no trust graph traversal, no anchor. A user
/// is eligible when any of the configured `verifier_ids` has issued them an
/// active endorsement with the given `topic` (typically `"identity_verified"`).
pub struct IdentityVerifiedConstraint {
    verifier_ids: Vec<Uuid>,
    topic: String,
}

impl IdentityVerifiedConstraint {
    /// Create a new constraint requiring an endorsement from one of the given verifiers.
    pub fn new(verifier_ids: Vec<Uuid>, topic: impl Into<String>) -> Self {
        Self {
            verifier_ids,
            topic: topic.into(),
        }
    }
}

#[async_trait]
impl RoomConstraint for IdentityVerifiedConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let verified = trust_reader
            .has_endorsement(user_id, &self.topic, &self.verifier_ids)
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;

        if verified {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(
                    "User has not completed identity verification from a recognised verifier"
                        .to_string(),
                ),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Factory helpers
// ---------------------------------------------------------------------------

fn parse_uuid_from_config(config: &serde_json::Value, key: &str) -> Result<Uuid, anyhow::Error> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| anyhow::anyhow!("constraint config requires valid UUID for '{key}'"))
}

fn get_f64_or_default(
    config: &serde_json::Value,
    key: &str,
    default: f64,
) -> Result<f64, anyhow::Error> {
    match config.get(key) {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(v) => v
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("config field '{key}' must be a number")),
    }
}

fn get_u32_or_default(
    config: &serde_json::Value,
    key: &str,
    default: u32,
) -> Result<u32, anyhow::Error> {
    match config.get(key) {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(v) => {
            let i = v
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("config field '{key}' must be an integer"))?;
            u32::try_from(i)
                .map_err(|_| anyhow::anyhow!("config field '{key}' value out of range for u32"))
        }
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Build a [`RoomConstraint`] from a constraint type string and JSONB config.
///
/// # Errors
///
/// Returns an error if `constraint_type` is not a known type, a config field has
/// the wrong type, or a value is out of the valid range.
pub fn build_constraint(
    constraint_type: &str,
    config: &serde_json::Value,
) -> Result<Box<dyn RoomConstraint>, anyhow::Error> {
    match constraint_type {
        "endorsed_by" => {
            let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
            Ok(Box::new(EndorsedByConstraint::new(anchor_id)))
        }
        "endorsed_by_user" => {
            let endorser_id = parse_uuid_from_config(config, "endorser_id")?;
            Ok(Box::new(EndorsedByUserConstraint::new(endorser_id)))
        }
        "community" => {
            let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
            let max_distance = get_f64_or_default(config, "max_distance", 5.0)?;
            let min_diversity = get_u32_or_default(config, "min_diversity", 2)?;

            Ok(Box::new(CommunityConstraint::new(
                anchor_id,
                max_distance,
                min_diversity,
            )?))
        }
        "congress" => {
            let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
            let min_diversity = get_u32_or_default(config, "min_diversity", 3)?;

            Ok(Box::new(CongressConstraint::new(anchor_id, min_diversity)?))
        }
        "identity_verified" => {
            let verifier_ids = config
                .get("verifier_ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                        .collect::<Vec<_>>()
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "identity_verified constraint requires verifier_ids array in config"
                    )
                })?;
            if verifier_ids.is_empty() {
                anyhow::bail!("identity_verified constraint requires at least one verifier_id");
            }
            Ok(Box::new(IdentityVerifiedConstraint::new(
                verifier_ids,
                "identity_verified",
            )))
        }
        other => Err(anyhow::anyhow!("unknown constraint type: {other}")),
    }
}

// ---------------------------------------------------------------------------
// ConstraintRegistry
// ---------------------------------------------------------------------------

/// A convenience wrapper around [`build_constraint`] that builds and evaluates
/// constraints in a single call.
///
/// This exists so that callers (e.g. room engines) don't need to import the
/// factory function and trait separately — they can hold a `ConstraintRegistry`
/// and call `check()` directly.
pub struct ConstraintRegistry;

impl ConstraintRegistry {
    /// Build a constraint from a type string + config and immediately evaluate
    /// it for `user_id`.
    ///
    /// This is a convenience method that combines [`build_constraint`] and
    /// [`RoomConstraint::check`] into a single call.
    ///
    /// Note: `trust_reader` is passed per-call rather than stored in the registry,
    /// allowing the registry to be constructed without a reader and shared across
    /// contexts with different trust graph views.
    ///
    /// # Errors
    ///
    /// Returns an error if constraint construction fails or the trust reader
    /// returns an infrastructure error.
    pub async fn check(
        &self,
        constraint_type: &str,
        config: &serde_json::Value,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        let constraint = build_constraint(constraint_type, config)?;
        constraint.check(user_id, trust_reader).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::TrustScoreSnapshot;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A configurable mock trust reader for testing constraints.
    struct MockTrustReader {
        /// Map of (subject, anchor) -> TrustScoreSnapshot.
        /// Uses `Uuid::nil()` as the key when anchor is None.
        scores: HashMap<(Uuid, Uuid), TrustScoreSnapshot>,
        /// Map of (subject, topic) -> bool for endorsement checks.
        endorsements: Mutex<HashMap<(Uuid, String), Vec<Uuid>>>,
    }

    impl MockTrustReader {
        fn new() -> Self {
            Self {
                scores: HashMap::new(),
                endorsements: Mutex::new(HashMap::new()),
            }
        }

        fn with_score(mut self, subject: Uuid, anchor: Uuid, snapshot: TrustScoreSnapshot) -> Self {
            self.scores.insert((subject, anchor), snapshot);
            self
        }

        fn with_endorsement(self, subject: Uuid, topic: &str, verifier_id: Uuid) -> Self {
            let mut endorsements = self.endorsements.lock().unwrap();
            endorsements
                .entry((subject, topic.to_string()))
                .or_default()
                .push(verifier_id);
            drop(endorsements);
            self
        }
    }

    #[async_trait]
    impl TrustGraphReader for MockTrustReader {
        async fn get_score(
            &self,
            subject: Uuid,
            anchor: Option<Uuid>,
        ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error> {
            let key = (subject, anchor.unwrap_or(Uuid::nil()));
            Ok(self.scores.get(&key).cloned())
        }

        async fn has_endorsement(
            &self,
            subject: Uuid,
            topic: &str,
            verifier_ids: &[Uuid],
        ) -> Result<bool, anyhow::Error> {
            let endorsements = self.endorsements.lock().unwrap();
            let key = (subject, topic.to_string());
            match endorsements.get(&key) {
                Some(verifiers) => Ok(verifier_ids.iter().any(|v| verifiers.contains(v))),
                None => Ok(false),
            }
        }
    }

    fn score(distance: f64, diversity: u32) -> TrustScoreSnapshot {
        TrustScoreSnapshot {
            trust_distance: distance,
            path_diversity: diversity,
            eigenvector_centrality: 0.5,
        }
    }

    // ── EndorsedByConstraint ──────────────────────────────────────────

    #[tokio::test]
    async fn endorsed_by_eligible_when_score_exists() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(2.0, 3));

        let constraint = EndorsedByConstraint::new(anchor);
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
        assert!(result.reason.is_none());
    }

    #[tokio::test]
    async fn endorsed_by_ineligible_when_no_score() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new();

        let constraint = EndorsedByConstraint::new(anchor);
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("not reachable"));
    }

    // ── CommunityConstraint ───────────────────────────────────────────

    #[tokio::test]
    async fn community_eligible_when_within_bounds() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(3.0, 4));

        let constraint = CommunityConstraint::new(anchor, 5.0, 2).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
        assert!(result.reason.is_none());
    }

    #[tokio::test]
    async fn community_ineligible_distance_too_high() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(10.0, 4));

        let constraint = CommunityConstraint::new(anchor, 5.0, 2).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        let reason = result.reason.unwrap();
        assert!(reason.contains("trust distance"));
        assert!(reason.contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn community_ineligible_diversity_too_low() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(2.0, 1));

        let constraint = CommunityConstraint::new(anchor, 5.0, 3).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        let reason = result.reason.unwrap();
        assert!(reason.contains("path diversity"));
        assert!(reason.contains("below minimum"));
    }

    #[tokio::test]
    async fn community_ineligible_both_failures_joined() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(10.0, 1));

        let constraint = CommunityConstraint::new(anchor, 5.0, 3).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        let reason = result.reason.unwrap();
        assert!(reason.contains("trust distance"));
        assert!(reason.contains("path diversity"));
        assert!(reason.contains("; "));
    }

    #[tokio::test]
    async fn community_ineligible_no_score() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new();

        let constraint = CommunityConstraint::new(anchor, 5.0, 2).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("no trust score"));
    }

    // ── CommunityConstraint validation ────────────────────────────────

    #[test]
    fn community_rejects_zero_max_distance() {
        let anchor = Uuid::new_v4();
        let err = CommunityConstraint::new(anchor, 0.0, 2).unwrap_err();
        assert!(err.to_string().contains("max_distance"));
    }

    #[test]
    fn community_rejects_negative_max_distance() {
        let anchor = Uuid::new_v4();
        let err = CommunityConstraint::new(anchor, -1.0, 2).unwrap_err();
        assert!(err.to_string().contains("max_distance"));
    }

    #[test]
    fn community_rejects_max_distance_over_100() {
        let anchor = Uuid::new_v4();
        let err = CommunityConstraint::new(anchor, 100.1, 2).unwrap_err();
        assert!(err.to_string().contains("max_distance"));
    }

    #[test]
    fn community_rejects_zero_min_diversity() {
        let anchor = Uuid::new_v4();
        let err = CommunityConstraint::new(anchor, 5.0, 0).unwrap_err();
        assert!(err.to_string().contains("min_diversity"));
    }

    // ── CongressConstraint ────────────────────────────────────────────

    #[tokio::test]
    async fn congress_eligible_when_diversity_sufficient() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(50.0, 5));

        let constraint = CongressConstraint::new(anchor, 3).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn congress_ineligible_diversity_too_low() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(1.0, 1));

        let constraint = CongressConstraint::new(anchor, 3).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("path diversity"));
    }

    #[tokio::test]
    async fn congress_ineligible_no_score() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let reader = MockTrustReader::new();

        let constraint = CongressConstraint::new(anchor, 3).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("no trust score"));
    }

    #[test]
    fn congress_rejects_zero_min_diversity() {
        let anchor = Uuid::new_v4();
        let err = CongressConstraint::new(anchor, 0).unwrap_err();
        assert!(err.to_string().contains("min_diversity"));
    }

    // ── IdentityVerifiedConstraint ────────────────────────────────────

    #[tokio::test]
    async fn identity_verified_eligible_with_endorsement() {
        let user = Uuid::new_v4();
        let verifier = Uuid::new_v4();
        let reader = MockTrustReader::new().with_endorsement(user, "identity_verified", verifier);

        let constraint = IdentityVerifiedConstraint::new(vec![verifier], "identity_verified");
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn identity_verified_ineligible_without_endorsement() {
        let user = Uuid::new_v4();
        let verifier = Uuid::new_v4();
        let reader = MockTrustReader::new();

        let constraint = IdentityVerifiedConstraint::new(vec![verifier], "identity_verified");
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        assert!(result.reason.unwrap().contains("identity verification"));
    }

    #[tokio::test]
    async fn identity_verified_ineligible_wrong_verifier() {
        let user = Uuid::new_v4();
        let verifier = Uuid::new_v4();
        let wrong_verifier = Uuid::new_v4();
        let reader =
            MockTrustReader::new().with_endorsement(user, "identity_verified", wrong_verifier);

        let constraint = IdentityVerifiedConstraint::new(vec![verifier], "identity_verified");
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
    }

    // ── EndorsedByUserConstraint ──────────────────────────────────────

    #[tokio::test]
    async fn endorsed_by_user_eligible_when_endorsement_exists() {
        let user = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let reader = MockTrustReader::new().with_endorsement(user, "trust", owner);

        let constraint = EndorsedByUserConstraint::new(owner);
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
        assert!(result.reason.is_none());
    }

    #[tokio::test]
    async fn endorsed_by_user_ineligible_without_endorsement() {
        let user = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let reader = MockTrustReader::new();

        let constraint = EndorsedByUserConstraint::new(owner);
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(!result.is_eligible);
        let reason = result.reason.unwrap();
        assert!(reason.contains("requires endorsement from room owner"));
        assert!(reason.contains(&owner.to_string()));
    }

    #[tokio::test]
    async fn endorsed_by_user_owner_is_always_eligible() {
        let owner = Uuid::new_v4();
        // No endorsements configured — owner passes regardless.
        let reader = MockTrustReader::new();

        let constraint = EndorsedByUserConstraint::new(owner);
        let result = constraint.check(owner, &reader).await.unwrap();
        assert!(result.is_eligible);
        assert!(result.reason.is_none());
    }

    // ── build_constraint factory ──────────────────────────────────────

    #[tokio::test]
    async fn build_endorsed_by_user() {
        let endorser = Uuid::new_v4();
        let user = Uuid::new_v4();
        let config = serde_json::json!({ "endorser_id": endorser.to_string() });
        let reader = MockTrustReader::new().with_endorsement(user, "trust", endorser);

        let constraint = build_constraint("endorsed_by_user", &config).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[test]
    fn build_endorsed_by_user_missing_endorser_errors() {
        let config = serde_json::json!({});
        let result = build_constraint("endorsed_by_user", &config);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("endorser_id"));
    }

    #[tokio::test]
    async fn build_endorsed_by() {
        let anchor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let config = serde_json::json!({ "anchor_id": anchor.to_string() });
        let reader = MockTrustReader::new().with_score(user, anchor, score(1.0, 1));

        let constraint = build_constraint("endorsed_by", &config).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn build_community_with_defaults() {
        let anchor = Uuid::new_v4();
        let user = Uuid::new_v4();
        // Default: max_distance=5.0, min_diversity=2
        let config = serde_json::json!({ "anchor_id": anchor.to_string() });
        let reader = MockTrustReader::new().with_score(user, anchor, score(3.0, 3));

        let constraint = build_constraint("community", &config).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn build_community_with_custom_values() {
        let anchor = Uuid::new_v4();
        let config = serde_json::json!({
            "anchor_id": anchor.to_string(),
            "max_distance": 10.0,
            "min_diversity": 5,
        });
        let constraint = build_constraint("community", &config).unwrap();
        // Just confirm it constructed successfully.
        let user = Uuid::new_v4();
        let reader = MockTrustReader::new().with_score(user, anchor, score(8.0, 6));
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn build_congress_with_defaults() {
        let anchor = Uuid::new_v4();
        let user = Uuid::new_v4();
        // Default: min_diversity=3
        let config = serde_json::json!({ "anchor_id": anchor.to_string() });
        let reader = MockTrustReader::new().with_score(user, anchor, score(1.0, 5));

        let constraint = build_constraint("congress", &config).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn build_identity_verified() {
        let verifier = Uuid::new_v4();
        let user = Uuid::new_v4();
        let config = serde_json::json!({ "verifier_ids": [verifier.to_string()] });
        let reader = MockTrustReader::new().with_endorsement(user, "identity_verified", verifier);

        let constraint = build_constraint("identity_verified", &config).unwrap();
        let result = constraint.check(user, &reader).await.unwrap();
        assert!(result.is_eligible);
    }

    #[test]
    fn build_unknown_type_errors() {
        let config = serde_json::json!({});
        let result = build_constraint("nonexistent", &config);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("unknown constraint type"));
    }

    #[test]
    fn build_endorsed_by_missing_anchor_errors() {
        let config = serde_json::json!({});
        let result = build_constraint("endorsed_by", &config);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("anchor_id"));
    }

    #[test]
    fn build_identity_verified_empty_verifiers_errors() {
        let config = serde_json::json!({ "verifier_ids": [] });
        let result = build_constraint("identity_verified", &config);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("at least one verifier_id"));
    }

    #[test]
    fn build_identity_verified_missing_verifiers_errors() {
        let config = serde_json::json!({});
        let result = build_constraint("identity_verified", &config);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("verifier_ids"));
    }

    // ── Error propagation ─────────────────────────────────────────────

    /// A mock trust reader that always returns an infrastructure error.
    struct FailingTrustReader;

    #[async_trait]
    impl TrustGraphReader for FailingTrustReader {
        async fn get_score(
            &self,
            _subject: Uuid,
            _anchor: Option<Uuid>,
        ) -> Result<Option<TrustScoreSnapshot>, anyhow::Error> {
            Err(anyhow::anyhow!("simulated trust reader failure"))
        }

        async fn has_endorsement(
            &self,
            _subject: Uuid,
            _topic: &str,
            _verifier_ids: &[Uuid],
        ) -> Result<bool, anyhow::Error> {
            Err(anyhow::anyhow!("simulated trust reader failure"))
        }
    }

    #[tokio::test]
    async fn endorsed_by_propagates_trust_reader_error() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let constraint = EndorsedByConstraint::new(anchor);
        let err = constraint.check(user, &FailingTrustReader).await.unwrap_err();
        assert!(err.to_string().contains("trust reader error"));
    }

    #[tokio::test]
    async fn community_propagates_trust_reader_error() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let constraint = CommunityConstraint::new(anchor, 5.0, 2).unwrap();
        let err = constraint.check(user, &FailingTrustReader).await.unwrap_err();
        assert!(err.to_string().contains("trust reader error"));
    }

    #[tokio::test]
    async fn congress_propagates_trust_reader_error() {
        let user = Uuid::new_v4();
        let anchor = Uuid::new_v4();
        let constraint = CongressConstraint::new(anchor, 3).unwrap();
        let err = constraint.check(user, &FailingTrustReader).await.unwrap_err();
        assert!(err.to_string().contains("trust reader error"));
    }

    #[tokio::test]
    async fn endorsed_by_user_propagates_trust_reader_error() {
        let user = Uuid::new_v4();
        let owner = Uuid::new_v4(); // distinct from user — avoids the owner short-circuit
        let constraint = EndorsedByUserConstraint::new(owner);
        let err = constraint.check(user, &FailingTrustReader).await.unwrap_err();
        assert!(err.to_string().contains("trust reader error"));
    }

    #[tokio::test]
    async fn identity_verified_propagates_trust_reader_error() {
        let user = Uuid::new_v4();
        let verifier = Uuid::new_v4();
        let constraint = IdentityVerifiedConstraint::new(vec![verifier], "identity_verified");
        let err = constraint.check(user, &FailingTrustReader).await.unwrap_err();
        assert!(err.to_string().contains("trust reader error"));
    }

    // ── ConstraintRegistry::check ─────────────────────────────────────

    #[tokio::test]
    async fn registry_check_delegates_correctly() {
        let registry = ConstraintRegistry;
        let anchor = Uuid::new_v4();
        let user = Uuid::new_v4();
        let config = serde_json::json!({ "anchor_id": anchor.to_string() });
        let reader = MockTrustReader::new().with_score(user, anchor, score(1.0, 1));

        let result = registry
            .check("endorsed_by", &config, user, &reader)
            .await
            .unwrap();
        assert!(result.is_eligible);
    }

    #[tokio::test]
    async fn registry_check_propagates_build_error() {
        let registry = ConstraintRegistry;
        let user = Uuid::new_v4();
        let config = serde_json::json!({});
        let reader = MockTrustReader::new();

        let err = registry
            .check("nonexistent", &config, user, &reader)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown constraint type"));
    }
}
