use serde::Serialize;
use tracing::{info, warn};
use uuid::Uuid;

/// Audit event types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AuditEvent {
    AuthFailure {
        account_id: Option<Uuid>,
        device_id: Option<Uuid>,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ip_address: Option<String>,
    },
    AuthSuccess {
        account_id: Uuid,
        device_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        ip_address: Option<String>,
    },
    EndorsementWrite {
        account_id: Uuid,
        device_id: Uuid,
        subject_type: String,
        subject_id: String,
        topic: String,
        magnitude: f64,
        confidence: f64,
    },
    EndorsementRevoke {
        account_id: Uuid,
        device_id: Uuid,
        endorsement_id: Uuid,
    },
    EndorsementRateLimited {
        account_id: Uuid,
        subject_type: String,
        subject_id: String,
        topic: String,
        limit_type: String,
    },
    DeviceRevoked {
        account_id: Uuid,
        device_id: Uuid,
        reason: Option<String>,
    },
    RecoveryApproval {
        account_id: Uuid,
        helper_account_id: Uuid,
        policy_id: Uuid,
    },
    RootRotation {
        account_id: Uuid,
        old_root_kid: String,
        new_root_kid: String,
    },
}

/// Emit an audit event
pub fn emit_audit_event(event: &AuditEvent) {
    match event {
        AuditEvent::AuthFailure { reason, .. } => {
            warn!(
                event = "auth.failure",
                reason = reason,
                audit_event = ?event,
                "Authentication failed"
            );
        }
        AuditEvent::AuthSuccess { account_id, .. } => {
            info!(
                event = "auth.success",
                account_id = %account_id,
                audit_event = ?event,
                "Authentication successful"
            );
        }
        AuditEvent::EndorsementWrite {
            account_id, topic, ..
        } => {
            info!(
                event = "endorsement.write",
                account_id = %account_id,
                topic = topic,
                audit_event = ?event,
                "Endorsement created"
            );
        }
        AuditEvent::EndorsementRevoke {
            account_id,
            endorsement_id,
            ..
        } => {
            info!(
                event = "endorsement.revoke",
                account_id = %account_id,
                endorsement_id = %endorsement_id,
                audit_event = ?event,
                "Endorsement revoked"
            );
        }
        AuditEvent::EndorsementRateLimited {
            account_id,
            limit_type,
            ..
        } => {
            warn!(
                event = "endorsement.rate_limited",
                account_id = %account_id,
                limit_type = limit_type,
                audit_event = ?event,
                "Endorsement rate limited"
            );
        }
        AuditEvent::DeviceRevoked { account_id, .. } => {
            info!(
                event = "device.revoked",
                account_id = %account_id,
                audit_event = ?event,
                "Device revoked"
            );
        }
        AuditEvent::RecoveryApproval { account_id, .. } => {
            info!(
                event = "recovery.approval",
                account_id = %account_id,
                audit_event = ?event,
                "Recovery approval submitted"
            );
        }
        AuditEvent::RootRotation { account_id, .. } => {
            info!(
                event = "root.rotation",
                account_id = %account_id,
                audit_event = ?event,
                "Root key rotated"
            );
        }
    }
}

/// Convenience function for auth failure
pub fn audit_auth_failure(account_id: Option<Uuid>, device_id: Option<Uuid>, reason: String) {
    emit_audit_event(&AuditEvent::AuthFailure {
        account_id,
        device_id,
        reason,
        ip_address: None,
    });
}

/// Convenience function for auth success
pub fn audit_auth_success(account_id: Uuid, device_id: Uuid) {
    emit_audit_event(&AuditEvent::AuthSuccess {
        account_id,
        device_id,
        ip_address: None,
    });
}

/// Convenience function for endorsement write
pub fn audit_endorsement_write(
    account_id: Uuid,
    device_id: Uuid,
    subject_type: String,
    subject_id: String,
    topic: String,
    magnitude: f64,
    confidence: f64,
) {
    emit_audit_event(&AuditEvent::EndorsementWrite {
        account_id,
        device_id,
        subject_type,
        subject_id,
        topic,
        magnitude,
        confidence,
    });
}

/// Convenience function for endorsement revoke
pub fn audit_endorsement_revoke(account_id: Uuid, device_id: Uuid, endorsement_id: Uuid) {
    emit_audit_event(&AuditEvent::EndorsementRevoke {
        account_id,
        device_id,
        endorsement_id,
    });
}

/// Convenience function for rate limiting
pub fn audit_rate_limit(
    account_id: Uuid,
    subject_type: String,
    subject_id: String,
    topic: String,
    limit_type: String,
) {
    emit_audit_event(&AuditEvent::EndorsementRateLimited {
        account_id,
        subject_type,
        subject_id,
        topic,
        limit_type,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::panic)]
    fn test_audit_event_serialization() {
        let event = AuditEvent::AuthFailure {
            account_id: Some(Uuid::new_v4()),
            device_id: None,
            reason: "Invalid signature".to_string(),
            ip_address: Some("127.0.0.1".to_string()),
        };

        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(e) => panic!("Failed to serialize: {e}"),
        };
        assert!(json.contains("auth_failure"));
        assert!(json.contains("Invalid signature"));
    }

    #[test]
    #[allow(clippy::panic)]
    fn test_endorsement_write_event() {
        let account_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let event = AuditEvent::EndorsementWrite {
            account_id,
            device_id,
            subject_type: "account".to_string(),
            subject_id: "test".to_string(),
            topic: "is_real_person".to_string(),
            magnitude: 1.0,
            confidence: 0.9,
        };

        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(e) => panic!("Failed to serialize: {e}"),
        };
        assert!(json.contains("endorsement_write"));
        assert!(json.contains("is_real_person"));
    }
}
