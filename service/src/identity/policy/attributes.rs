use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Context containing all attributes needed for authorization decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeContext {
    pub account_id: Uuid,
    pub device_id: Option<Uuid>,
    pub tier: String,
    pub verification_state: String,
    pub reputation_score: f64,
    pub posture_label: Option<String>,
    pub device_revoked: bool,
    pub delegation_active: bool,
}

/// Fetch attributes for an account and optionally a device
pub async fn fetch_attributes(
    pool: &PgPool,
    account_id: Uuid,
    device_id: Option<Uuid>,
) -> Result<AttributeContext> {
    // Fetch account info
    let account = sqlx::query!(
        r#"
        SELECT tier, verification_state
        FROM accounts
        WHERE id = $1
        "#,
        account_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to fetch account")?;

    // Fetch reputation score if it exists
    let reputation = sqlx::query!(
        r#"
        SELECT score, posture_label
        FROM reputation_scores
        WHERE account_id = $1
        "#,
        account_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch reputation")?;

    let reputation_score = reputation.as_ref().map(|r| r.score).unwrap_or(0.0);
    let posture_label = reputation.and_then(|r| r.posture_label);

    // Fetch device info if device_id provided
    let (device_revoked, delegation_active) = if let Some(device_id) = device_id {
        let device = sqlx::query!(
            r#"
            SELECT d.revoked_at,
                   EXISTS(
                       SELECT 1
                       FROM device_delegations dd
                       WHERE dd.account_id = $1
                         AND dd.device_id = $2
                         AND dd.revoked_at IS NULL
                         AND (dd.expires_at IS NULL OR dd.expires_at > NOW())
                   ) as "has_active_delegation!"
            FROM devices d
            WHERE d.account_id = $1 AND d.id = $2
            "#,
            account_id,
            device_id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to fetch device")?;

        match device {
            Some(d) => (d.revoked_at.is_some(), d.has_active_delegation),
            None => (false, false),
        }
    } else {
        (false, true) // No device context, assume not revoked and active
    };

    Ok(AttributeContext {
        account_id,
        device_id,
        tier: account.tier,
        verification_state: account.verification_state,
        reputation_score,
        posture_label,
        device_revoked,
        delegation_active,
    })
}

/// Compute security posture score based on device activity and factors
pub async fn compute_security_posture_score(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<f64> {
    // Count active devices
    let active_devices = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::int as count
        FROM devices
        WHERE account_id = $1
          AND revoked_at IS NULL
        "#,
        account_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to count active devices")?
    .unwrap_or(0);

    // Count devices with active delegations
    let devices_with_delegations = sqlx::query_scalar!(
        r#"
        SELECT COUNT(DISTINCT device_id)::int as count
        FROM device_delegations
        WHERE account_id = $1
          AND revoked_at IS NULL
          AND (expires_at IS NULL OR expires_at > NOW())
        "#,
        account_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to count delegated devices")?
    .unwrap_or(0);

    // Simple heuristic: base score + bonuses for active devices
    let mut score: f64 = 10.0;

    // Bonus for having at least one device
    if active_devices > 0 {
        score += 20.0;
    }

    // Bonus for having active delegations
    if devices_with_delegations > 0 {
        score += 30.0;
    }

    // Bonus for multiple devices (up to 3)
    if active_devices >= 2 {
        score += 20.0;
    }
    if active_devices >= 3 {
        score += 20.0;
    }

    Ok(score.min(100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_context_creation() {
        let ctx = AttributeContext {
            account_id: Uuid::new_v4(),
            device_id: Some(Uuid::new_v4()),
            tier: "verified".to_string(),
            verification_state: "verified".to_string(),
            reputation_score: 75.0,
            posture_label: Some("strong".to_string()),
            device_revoked: false,
            delegation_active: true,
        };

        assert_eq!(ctx.tier, "verified");
        assert_eq!(ctx.reputation_score, 75.0);
        assert!(!ctx.device_revoked);
    }
}
