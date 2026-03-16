//! Weight computation for endorsements based on delivery method and relationship depth.
//!
//! Final weight = `base_weight(delivery_method)` × `depth_multiplier(relationship_depth)`,
//! clamped to (0, 1.0].

/// Base weight for each delivery method (ADR-023).
#[must_use]
pub fn base_weight(delivery_method: &str) -> f32 {
    match delivery_method {
        "video" => 0.7,
        "text" | "messaging" => 0.4,
        "email" => 0.2,
        // "qr" and unknown methods default to full weight
        _ => 1.0,
    }
}

/// Multiplier for relationship depth (ADR-023).
#[must_use]
pub fn depth_multiplier(relationship_depth: Option<&str>) -> f32 {
    match relationship_depth {
        Some("months") => 0.7,
        Some("acquaintance") => 0.5,
        // "years", None, or unrecognised values default to no reduction
        _ => 1.0,
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
