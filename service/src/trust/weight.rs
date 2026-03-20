//! Weight computation for endorsements based on delivery method and relationship depth.
//!
//! Final weight = `base_weight(delivery_method)` × `depth_multiplier(relationship_depth)`,
//! clamped to (0, 1.0].

/// Valid delivery method strings (must match the `trust_invites.delivery_method` DB check constraint).
pub const VALID_DELIVERY_METHODS: &[&str] = &["qr", "email", "video", "text", "messaging"];

/// Valid relationship depth strings (must match the `trust_invites.relationship_depth` DB check constraint).
pub const VALID_RELATIONSHIP_DEPTHS: &[&str] = &["years", "months", "acquaintance"];

/// Base weight for each delivery method (ADR-023).
///
/// # Panics
///
/// Panics if `delivery_method` is not a recognised value. Callers must validate
/// input before invoking this function (e.g. at the HTTP boundary).
#[must_use]
pub fn base_weight(delivery_method: &str) -> f32 {
    match delivery_method {
        "qr" => 1.0,
        "video" => 0.7,
        "text" | "messaging" => 0.4,
        "email" => 0.2,
        other => unreachable!("unrecognised delivery_method: {other:?}; validate before calling"),
    }
}

/// Multiplier for relationship depth (ADR-023).
///
/// # Panics
///
/// Panics if `relationship_depth` is `Some` with an unrecognised value. Callers
/// must validate input before invoking this function (e.g. at the HTTP boundary).
#[must_use]
pub fn depth_multiplier(relationship_depth: Option<&str>) -> f32 {
    match relationship_depth {
        Some("years") | None => 1.0,
        Some("months") => 0.7,
        Some("acquaintance") => 0.5,
        Some(other) => {
            unreachable!("unrecognised relationship_depth: {other:?}; validate before calling")
        }
    }
}

/// Compute the final endorsement weight, clamped to (0.0, 1.0].
#[must_use]
pub fn compute_endorsement_weight(delivery_method: &str, relationship_depth: Option<&str>) -> f32 {
    let raw = base_weight(delivery_method) * depth_multiplier(relationship_depth);
    raw.clamp(f32::MIN_POSITIVE, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_years_is_full_weight() {
        assert!((compute_endorsement_weight("qr", Some("years")) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn email_acquaintance_is_minimum() {
        // 0.2 * 0.5 = 0.1
        let w = compute_endorsement_weight("email", Some("acquaintance"));
        assert!((w - 0.1).abs() < 0.001, "expected ~0.1, got {w}");
    }

    #[test]
    fn video_months_rounds_correctly() {
        // 0.7 * 0.7 = 0.49
        let w = compute_endorsement_weight("video", Some("months"));
        assert!((w - 0.49).abs() < 0.001, "expected ~0.49, got {w}");
    }

    #[test]
    fn no_depth_defaults_to_no_reduction() {
        let w = compute_endorsement_weight("qr", None);
        assert!((w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn result_is_always_positive() {
        for method in &["qr", "video", "text", "messaging", "email"] {
            for depth in &[None, Some("years"), Some("months"), Some("acquaintance")] {
                let w = compute_endorsement_weight(method, *depth);
                assert!(
                    w > 0.0,
                    "weight must be positive for method={method} depth={depth:?}"
                );
                assert!(
                    w <= 1.0,
                    "weight must not exceed 1.0 for method={method} depth={depth:?}"
                );
            }
        }
    }
}
