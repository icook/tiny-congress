//! Weight computation for endorsements based on delivery method and relationship depth.
//!
//! Final weight = `base_weight(delivery_method)` × `depth_multiplier(relationship_depth)`,
//! clamped to (0, 1.0].

/// Delivery method for a trust invite.
///
/// Variants must match the `trust_invites.delivery_method` DB check constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryMethod {
    Qr,
    Email,
    Video,
    Text,
    Messaging,
}

impl DeliveryMethod {
    /// Return the canonical string representation used in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Qr => "qr",
            Self::Email => "email",
            Self::Video => "video",
            Self::Text => "text",
            Self::Messaging => "messaging",
        }
    }
}

/// Relationship depth for a trust invite.
///
/// Variants must match the `trust_invites.relationship_depth` DB check constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipDepth {
    Years,
    Months,
    Acquaintance,
}

impl RelationshipDepth {
    /// Return the canonical string representation used in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Years => "years",
            Self::Months => "months",
            Self::Acquaintance => "acquaintance",
        }
    }
}

/// Base weight for each delivery method (ADR-023).
#[must_use]
pub const fn base_weight(delivery_method: DeliveryMethod) -> f32 {
    match delivery_method {
        DeliveryMethod::Qr => 1.0,
        DeliveryMethod::Video => 0.7,
        DeliveryMethod::Text | DeliveryMethod::Messaging => 0.4,
        DeliveryMethod::Email => 0.2,
    }
}

/// Multiplier for relationship depth (ADR-023).
#[must_use]
pub const fn depth_multiplier(relationship_depth: Option<RelationshipDepth>) -> f32 {
    match relationship_depth {
        Some(RelationshipDepth::Years) | None => 1.0,
        Some(RelationshipDepth::Months) => 0.7,
        Some(RelationshipDepth::Acquaintance) => 0.5,
    }
}

/// Compute the final endorsement weight, clamped to (0.0, 1.0].
#[must_use]
pub fn compute_endorsement_weight(
    delivery_method: DeliveryMethod,
    relationship_depth: Option<RelationshipDepth>,
) -> f32 {
    let raw = base_weight(delivery_method) * depth_multiplier(relationship_depth);
    raw.clamp(f32::MIN_POSITIVE, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_years_is_full_weight() {
        assert!(
            (compute_endorsement_weight(DeliveryMethod::Qr, Some(RelationshipDepth::Years)) - 1.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn email_acquaintance_is_minimum() {
        // 0.2 * 0.5 = 0.1
        let w = compute_endorsement_weight(
            DeliveryMethod::Email,
            Some(RelationshipDepth::Acquaintance),
        );
        assert!((w - 0.1).abs() < 0.001, "expected ~0.1, got {w}");
    }

    #[test]
    fn video_months_rounds_correctly() {
        // 0.7 * 0.7 = 0.49
        let w = compute_endorsement_weight(DeliveryMethod::Video, Some(RelationshipDepth::Months));
        assert!((w - 0.49).abs() < 0.001, "expected ~0.49, got {w}");
    }

    #[test]
    fn no_depth_defaults_to_no_reduction() {
        let w = compute_endorsement_weight(DeliveryMethod::Qr, None);
        assert!((w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn result_is_always_positive() {
        let methods = [
            DeliveryMethod::Qr,
            DeliveryMethod::Video,
            DeliveryMethod::Text,
            DeliveryMethod::Messaging,
            DeliveryMethod::Email,
        ];
        let depths = [
            None,
            Some(RelationshipDepth::Years),
            Some(RelationshipDepth::Months),
            Some(RelationshipDepth::Acquaintance),
        ];
        for method in methods {
            for depth in depths {
                let w = compute_endorsement_weight(method, depth);
                assert!(
                    w > 0.0,
                    "weight must be positive for method={method:?} depth={depth:?}"
                );
                assert!(
                    w <= 1.0,
                    "weight must not exceed 1.0 for method={method:?} depth={depth:?}"
                );
            }
        }
    }

    /// Verify the DB-facing string representation for each `DeliveryMethod` variant.
    ///
    /// These strings must match the `trust_invites.delivery_method` CHECK constraint.
    /// If a variant is renamed or its `as_str()` value changes, this test catches it
    /// before a migration is needed to fix a constraint violation.
    #[test]
    fn delivery_method_as_str_matches_db_constraint() {
        assert_eq!(DeliveryMethod::Qr.as_str(), "qr");
        assert_eq!(DeliveryMethod::Email.as_str(), "email");
        assert_eq!(DeliveryMethod::Video.as_str(), "video");
        assert_eq!(DeliveryMethod::Text.as_str(), "text");
        assert_eq!(DeliveryMethod::Messaging.as_str(), "messaging");
    }

    /// Verify the DB-facing string representation for each `RelationshipDepth` variant.
    ///
    /// These strings must match the `trust_invites.relationship_depth` CHECK constraint.
    #[test]
    fn relationship_depth_as_str_matches_db_constraint() {
        assert_eq!(RelationshipDepth::Years.as_str(), "years");
        assert_eq!(RelationshipDepth::Months.as_str(), "months");
        assert_eq!(RelationshipDepth::Acquaintance.as_str(), "acquaintance");
    }

    /// Pin the exact base weights for Text and Messaging (ADR-023).
    ///
    /// Both methods share the same 0.4 base weight. If someone differentiates
    /// them or changes the value, the `result_is_always_positive` test would
    /// not catch it (it only checks > 0 and <= 1.0).
    #[test]
    fn text_no_depth_is_0_4() {
        let w = compute_endorsement_weight(DeliveryMethod::Text, None);
        assert!((w - 0.4).abs() < 0.001, "expected ~0.4, got {w}");
    }

    #[test]
    fn messaging_no_depth_is_0_4() {
        let w = compute_endorsement_weight(DeliveryMethod::Messaging, None);
        assert!((w - 0.4).abs() < 0.001, "expected ~0.4, got {w}");
    }

    #[test]
    fn text_and_messaging_have_equal_base_weight() {
        let w_text = compute_endorsement_weight(DeliveryMethod::Text, None);
        let w_msg = compute_endorsement_weight(DeliveryMethod::Messaging, None);
        assert_eq!(
            w_text, w_msg,
            "Text and Messaging must share the same base weight per ADR-023"
        );
    }
}
