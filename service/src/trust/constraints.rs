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
#[async_trait]
pub trait RoomConstraint: Send + Sync {
    /// Check whether `user_id` may participate in a room whose anchor/context is `room_anchor_id`.
    async fn check(
        &self,
        user_id: Uuid,
        room_anchor_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error>;
}

// ---------------------------------------------------------------------------
// EndorsedByConstraint
// ---------------------------------------------------------------------------

/// User must appear in the trust graph reachable from `room_anchor_id`.
pub struct EndorsedByConstraint;

#[async_trait]
impl RoomConstraint for EndorsedByConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        room_anchor_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(room_anchor_id))
            .await
            .map_err(|e| anyhow::anyhow!("trust repo error: {e}"))?;

        // A snapshot can exist without `trust_distance` if only centrality was computed.
        // `trust_distance` being present means the user is reachable from the anchor —
        // the distance was set during graph traversal, so Some(d) = reachable.
        let is_eligible = snapshot.as_ref().and_then(|s| s.trust_distance).is_some();

        if is_eligible {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some("not reachable from room anchor in trust graph".to_string()),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// CommunityConstraint
// ---------------------------------------------------------------------------

/// User must have `trust_distance <= max_distance` AND `path_diversity >= min_diversity`.
pub struct CommunityConstraint {
    max_distance: f32,
    min_diversity: i32,
}

impl CommunityConstraint {
    /// Create a new `CommunityConstraint`, validating that values are in range.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_distance` is not in `(0.0, 100.0]` or `min_diversity < 1`.
    pub fn new(max_distance: f32, min_diversity: i32) -> Result<Self, anyhow::Error> {
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
        room_anchor_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(room_anchor_id))
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
    min_diversity: i32,
}

impl CongressConstraint {
    /// Create a new `CongressConstraint`, validating that `min_diversity >= 1`.
    ///
    /// # Errors
    ///
    /// Returns an error if `min_diversity < 1`.
    pub fn new(min_diversity: i32) -> Result<Self, anyhow::Error> {
        if min_diversity < 1 {
            return Err(anyhow::anyhow!(
                "min_diversity must be >= 1, got {min_diversity}"
            ));
        }
        Ok(Self { min_diversity })
    }
}

#[async_trait]
impl RoomConstraint for CongressConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        room_anchor_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(room_anchor_id))
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
// Factory helpers
// ---------------------------------------------------------------------------

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
        "endorsed_by" => Ok(Box::new(EndorsedByConstraint)),
        "community" => {
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
                max_distance,
                min_diversity,
            )?))
        }
        "congress" => {
            let min_diversity_i64 = get_i64_or_default(config, "min_diversity", 3)?;
            let min_diversity = i32::try_from(min_diversity_i64)
                .map_err(|_| anyhow::anyhow!("min_diversity value out of range for i32"))?;

            Ok(Box::new(CongressConstraint::new(min_diversity)?))
        }
        other => Err(anyhow::anyhow!("unknown constraint type: {other}")),
    }
}
