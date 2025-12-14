use super::ast::{Condition, Operator, Policy, Value};
use super::attributes::AttributeContext;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Action types for authorization checks
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    CreateEndorsement,
    RevokeEndorsement,
    AddDevice,
    RevokeDevice,
    CreateRecoveryPolicy,
    ApproveRecovery,
    RotateRoot,
}

impl Action {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::CreateEndorsement => "create_endorsement",
            Action::RevokeEndorsement => "revoke_endorsement",
            Action::AddDevice => "add_device",
            Action::RevokeDevice => "revoke_device",
            Action::CreateRecoveryPolicy => "create_recovery_policy",
            Action::ApproveRecovery => "approve_recovery",
            Action::RotateRoot => "rotate_root",
        }
    }
}

/// Resource context for authorization
#[derive(Debug, Clone)]
pub struct ResourceContext {
    pub resource_type: String,
    pub resource_id: Option<String>,
}

/// Evaluate a condition against an attribute context
fn evaluate_condition(condition: &Condition, attrs: &HashMap<String, Value>) -> Result<bool> {
    match condition.op {
        Operator::And => {
            for c in &condition.conditions {
                if !evaluate_condition(c, attrs)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Operator::Or => {
            for c in &condition.conditions {
                if evaluate_condition(c, attrs)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Operator::Not => {
            if condition.conditions.len() != 1 {
                return Err(anyhow!("NOT operator requires exactly one condition"));
            }
            Ok(!evaluate_condition(&condition.conditions[0], attrs)?)
        }
        Operator::Eq => {
            let field = condition
                .field
                .as_ref()
                .ok_or_else(|| anyhow!("EQ requires field"))?;
            let expected = condition
                .value
                .as_ref()
                .ok_or_else(|| anyhow!("EQ requires value"))?;
            let actual = attrs.get(field);
            Ok(actual == Some(expected))
        }
        Operator::Ne => {
            let field = condition
                .field
                .as_ref()
                .ok_or_else(|| anyhow!("NE requires field"))?;
            let expected = condition
                .value
                .as_ref()
                .ok_or_else(|| anyhow!("NE requires value"))?;
            let actual = attrs.get(field);
            Ok(actual != Some(expected))
        }
        Operator::Gt | Operator::Gte | Operator::Lt | Operator::Lte => {
            let field = condition
                .field
                .as_ref()
                .ok_or_else(|| anyhow!("Comparison requires field"))?;
            let expected = condition
                .value
                .as_ref()
                .ok_or_else(|| anyhow!("Comparison requires value"))?;
            let actual = attrs
                .get(field)
                .ok_or_else(|| anyhow!("Field not found: {field}"))?;

            match (actual, expected) {
                (Value::Number(a), Value::Number(e)) => Ok(match condition.op {
                    Operator::Gt => a > e,
                    Operator::Gte => a >= e,
                    Operator::Lt => a < e,
                    Operator::Lte => a <= e,
                    _ => unreachable!(),
                }),
                _ => Err(anyhow!("Comparison requires numeric values")),
            }
        }
        Operator::In => {
            let field = condition
                .field
                .as_ref()
                .ok_or_else(|| anyhow!("IN requires field"))?;
            let expected_list = condition
                .value
                .as_ref()
                .ok_or_else(|| anyhow!("IN requires value"))?;
            let actual = attrs.get(field);

            match expected_list {
                Value::Array(list) => Ok(actual.map(|v| list.contains(v)).unwrap_or(false)),
                _ => Err(anyhow!("IN operator requires array value")),
            }
        }
    }
}

/// Evaluate a policy against an attribute context
fn evaluate_policy(policy: &Policy, attrs: &HashMap<String, Value>) -> Result<bool> {
    evaluate_condition(&policy.condition, attrs)
}

/// Convert AttributeContext to a HashMap for evaluation
fn attributes_to_map(ctx: &AttributeContext) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    map.insert(
        "account_id".to_string(),
        Value::String(ctx.account_id.to_string()),
    );
    if let Some(device_id) = ctx.device_id {
        map.insert(
            "device_id".to_string(),
            Value::String(device_id.to_string()),
        );
    }
    map.insert("tier".to_string(), Value::String(ctx.tier.clone()));
    map.insert(
        "verification_state".to_string(),
        Value::String(ctx.verification_state.clone()),
    );
    map.insert(
        "reputation_score".to_string(),
        Value::Number(ctx.reputation_score),
    );
    if let Some(posture) = &ctx.posture_label {
        map.insert("posture_label".to_string(), Value::String(posture.clone()));
    }
    map.insert(
        "device_revoked".to_string(),
        Value::Bool(ctx.device_revoked),
    );
    map.insert(
        "delegation_active".to_string(),
        Value::Bool(ctx.delegation_active),
    );
    map
}

/// Get hardcoded policy for a given action
fn get_hardcoded_policy(action: &Action) -> Policy {
    match action {
        Action::CreateEndorsement => Policy {
            name: "create_endorsement".to_string(),
            description: Some("Allow endorsement creation if device is active".to_string()),
            condition: Condition::and(vec![
                Condition::eq("device_revoked", Value::Bool(false)),
                Condition::eq("delegation_active", Value::Bool(true)),
            ]),
        },
        Action::RevokeEndorsement => Policy {
            name: "revoke_endorsement".to_string(),
            description: Some("Allow endorsement revocation if device is active".to_string()),
            condition: Condition::and(vec![
                Condition::eq("device_revoked", Value::Bool(false)),
                Condition::eq("delegation_active", Value::Bool(true)),
            ]),
        },
        Action::AddDevice => Policy {
            name: "add_device".to_string(),
            description: Some("Allow device add if tier is verified or higher".to_string()),
            condition: Condition::in_list(
                "tier",
                vec![
                    Value::String("verified".to_string()),
                    Value::String("bonded".to_string()),
                    Value::String("vouched".to_string()),
                ],
            ),
        },
        Action::RevokeDevice => Policy {
            name: "revoke_device".to_string(),
            description: Some("Always allow device revocation by account owner".to_string()),
            condition: Condition::or(vec![
                Condition::eq("tier", Value::String("anonymous".to_string())),
                Condition::in_list(
                    "tier",
                    vec![
                        Value::String("verified".to_string()),
                        Value::String("bonded".to_string()),
                        Value::String("vouched".to_string()),
                    ],
                ),
            ]),
        },
        Action::CreateRecoveryPolicy => Policy {
            name: "create_recovery_policy".to_string(),
            description: Some("Allow recovery policy creation for verified accounts".to_string()),
            condition: Condition::in_list(
                "tier",
                vec![
                    Value::String("verified".to_string()),
                    Value::String("bonded".to_string()),
                    Value::String("vouched".to_string()),
                ],
            ),
        },
        Action::ApproveRecovery => Policy {
            name: "approve_recovery".to_string(),
            description: Some("Allow recovery approval if device is active".to_string()),
            condition: Condition::and(vec![
                Condition::eq("device_revoked", Value::Bool(false)),
                Condition::eq("delegation_active", Value::Bool(true)),
            ]),
        },
        Action::RotateRoot => Policy {
            name: "rotate_root".to_string(),
            description: Some("Allow root rotation if posture is adequate".to_string()),
            condition: Condition::gte("reputation_score", Value::Number(10.0)),
        },
    }
}

/// Main authorization entry point
pub fn authorize(
    action: &Action,
    context: &AttributeContext,
    _resource: Option<&ResourceContext>,
) -> Result<bool> {
    let policy = get_hardcoded_policy(action);
    let attrs = attributes_to_map(context);
    evaluate_policy(&policy, &attrs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_context() -> AttributeContext {
        AttributeContext {
            account_id: Uuid::new_v4(),
            device_id: Some(Uuid::new_v4()),
            tier: "verified".to_string(),
            verification_state: "verified".to_string(),
            reputation_score: 75.0,
            posture_label: Some("strong".to_string()),
            device_revoked: false,
            delegation_active: true,
        }
    }

    #[test]
    fn test_evaluate_condition_eq() -> Result<()> {
        let mut attrs = HashMap::new();
        attrs.insert("tier".to_string(), Value::String("verified".to_string()));

        let cond = Condition::eq("tier", Value::String("verified".to_string()));
        assert!(evaluate_condition(&cond, &attrs)?);

        let cond = Condition::eq("tier", Value::String("anonymous".to_string()));
        assert!(!evaluate_condition(&cond, &attrs)?);
        Ok(())
    }

    #[test]
    fn test_evaluate_condition_gte() -> Result<()> {
        let mut attrs = HashMap::new();
        attrs.insert("reputation_score".to_string(), Value::Number(75.0));

        let cond = Condition::gte("reputation_score", Value::Number(50.0));
        assert!(evaluate_condition(&cond, &attrs)?);

        let cond = Condition::gte("reputation_score", Value::Number(100.0));
        assert!(!evaluate_condition(&cond, &attrs)?);
        Ok(())
    }

    #[test]
    fn test_evaluate_condition_and() -> Result<()> {
        let mut attrs = HashMap::new();
        attrs.insert("tier".to_string(), Value::String("verified".to_string()));
        attrs.insert("reputation_score".to_string(), Value::Number(75.0));

        let cond = Condition::and(vec![
            Condition::eq("tier", Value::String("verified".to_string())),
            Condition::gte("reputation_score", Value::Number(50.0)),
        ]);
        assert!(evaluate_condition(&cond, &attrs)?);

        let cond = Condition::and(vec![
            Condition::eq("tier", Value::String("verified".to_string())),
            Condition::gte("reputation_score", Value::Number(100.0)),
        ]);
        assert!(!evaluate_condition(&cond, &attrs)?);
        Ok(())
    }

    #[test]
    fn test_evaluate_condition_in() -> Result<()> {
        let mut attrs = HashMap::new();
        attrs.insert("tier".to_string(), Value::String("verified".to_string()));

        let cond = Condition::in_list(
            "tier",
            vec![
                Value::String("verified".to_string()),
                Value::String("bonded".to_string()),
            ],
        );
        assert!(evaluate_condition(&cond, &attrs)?);

        let cond = Condition::in_list("tier", vec![Value::String("anonymous".to_string())]);
        assert!(!evaluate_condition(&cond, &attrs)?);
        Ok(())
    }

    #[test]
    fn test_authorize_create_endorsement() -> Result<()> {
        let ctx = create_test_context();
        assert!(authorize(&Action::CreateEndorsement, &ctx, None)?);

        let mut revoked_ctx = create_test_context();
        revoked_ctx.device_revoked = true;
        assert!(!authorize(&Action::CreateEndorsement, &revoked_ctx, None)?);

        let mut inactive_ctx = create_test_context();
        inactive_ctx.delegation_active = false;
        assert!(!authorize(&Action::CreateEndorsement, &inactive_ctx, None)?);
        Ok(())
    }

    #[test]
    fn test_authorize_add_device() -> Result<()> {
        let ctx = create_test_context();
        assert!(authorize(&Action::AddDevice, &ctx, None)?);

        let mut anonymous_ctx = create_test_context();
        anonymous_ctx.tier = "anonymous".to_string();
        assert!(!authorize(&Action::AddDevice, &anonymous_ctx, None)?);
        Ok(())
    }

    #[test]
    fn test_authorize_rotate_root() -> Result<()> {
        let ctx = create_test_context();
        assert!(authorize(&Action::RotateRoot, &ctx, None)?);

        let mut low_rep_ctx = create_test_context();
        low_rep_ctx.reputation_score = 5.0;
        assert!(!authorize(&Action::RotateRoot, &low_rep_ctx, None)?);
        Ok(())
    }
}
