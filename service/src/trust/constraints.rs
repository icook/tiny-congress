//! Room constraint trait and preset implementations.

use async_trait::async_trait;
use uuid::Uuid;

use crate::trust::repo::TrustRepo;

/// Result of a room eligibility check.
pub struct Eligibility {
    pub is_eligible: bool,
    /// Human-readable explanation if ineligible.
    pub reason: Option<String>,
}

/// Pluggable constraint that determines whether a user may participate in a room.
///
/// Constraints are self-contained policy objects: all configuration (including
/// anchor identity for trust-graph constraints) is captured at construction
/// time via `build_constraint`. Callers only need `user_id` and the repo.
#[async_trait]
pub trait RoomConstraint: Send + Sync {
    async fn check(
        &self,
        user_id: Uuid,
        trust_repo: &dyn TrustRepo,
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
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust repo error: {e}"))?;

        // `trust_distance` being present means the user is reachable from the anchor.
        match snapshot.and_then(|s| s.trust_distance) {
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
pub struct CommunityConstraint {
    anchor_id: Uuid,
    max_distance: f32,
    min_diversity: i32,
}

impl CommunityConstraint {
    /// Create a new `CommunityConstraint`, validating that values are in range.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_distance` is not in `(0.0, 100.0]` or `min_diversity < 1`.
    pub fn new(
        anchor_id: Uuid,
        max_distance: f32,
        min_diversity: i32,
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
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust repo error: {e}"))?;

        let Some(snap) = snapshot else {
            return Ok(Eligibility {
                is_eligible: false,
                reason: Some("no trust score found for user".to_string()),
            });
        };

        let mut failures: Vec<String> = Vec::new();

        match snap.trust_distance {
            None => failures.push("trust distance not computed".to_string()),
            Some(d) if d > self.max_distance => {
                failures.push(format!(
                    "trust distance {d:.2} exceeds maximum {:.2}",
                    self.max_distance
                ));
            }
            _ => {}
        }

        match snap.path_diversity {
            None => failures.push("path diversity not computed".to_string()),
            Some(p) if p < self.min_diversity => {
                failures.push(format!(
                    "path diversity {p} is below minimum {}",
                    self.min_diversity
                ));
            }
            _ => {}
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
pub struct CongressConstraint {
    anchor_id: Uuid,
    min_diversity: i32,
}

impl CongressConstraint {
    /// Create a new `CongressConstraint`, validating that `min_diversity >= 1`.
    ///
    /// # Errors
    ///
    /// Returns an error if `min_diversity < 1`.
    pub fn new(anchor_id: Uuid, min_diversity: i32) -> Result<Self, anyhow::Error> {
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
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(self.anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust repo error: {e}"))?;

        let Some(snap) = snapshot else {
            return Ok(Eligibility {
                is_eligible: false,
                reason: Some("no trust score found for user".to_string()),
            });
        };

        match snap.path_diversity {
            None => Ok(Eligibility {
                is_eligible: false,
                reason: Some("path diversity not computed".to_string()),
            }),
            Some(p) if p < self.min_diversity => Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "path diversity {p} is below minimum {}",
                    self.min_diversity
                )),
            }),
            _ => Ok(Eligibility {
                is_eligible: true,
                reason: None,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// IdentityVerifiedConstraint (Layer 1)
// ---------------------------------------------------------------------------

/// Layer 1 constraint: checks identity attestation from recognised verifiers.
///
/// Queries `reputation__endorsements` directly — no trust graph traversal,
/// no anchor. A user is eligible when any of the configured `verifier_ids`
/// has issued them an active endorsement with the given `topic`
/// (typically `"identity_verified"`).
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
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let verified = trust_repo
            .has_identity_endorsement(user_id, &self.verifier_ids, &self.topic)
            .await
            .map_err(|e| anyhow::anyhow!("trust repo error: {e}"))?;

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

fn get_i64_or_default(
    config: &serde_json::Value,
    key: &str,
    default: i64,
) -> Result<i64, anyhow::Error> {
    match config.get(key) {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(v) => v
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("config field '{key}' must be an integer")),
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
        "community" => {
            let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
            let max_distance_f64 = get_f64_or_default(config, "max_distance", 5.0)?;
            if max_distance_f64 > f64::from(f32::MAX) {
                return Err(anyhow::anyhow!("max_distance value too large"));
            }
            // Safety: value has been verified to fit in f32 by the bounds check above.
            #[allow(clippy::cast_possible_truncation)]
            let max_distance = max_distance_f64 as f32;

            let min_diversity_i64 = get_i64_or_default(config, "min_diversity", 2)?;
            let min_diversity = i32::try_from(min_diversity_i64)
                .map_err(|_| anyhow::anyhow!("min_diversity value out of range for i32"))?;

            Ok(Box::new(CommunityConstraint::new(
                anchor_id,
                max_distance,
                min_diversity,
            )?))
        }
        "congress" => {
            let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
            let min_diversity_i64 = get_i64_or_default(config, "min_diversity", 3)?;
            let min_diversity = i32::try_from(min_diversity_i64)
                .map_err(|_| anyhow::anyhow!("min_diversity value out of range for i32"))?;

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
