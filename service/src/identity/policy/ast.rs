use serde::{Deserialize, Serialize};

/// Represents a value in a policy expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::float_cmp)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<Value>),
}

/// Comparison and logical operators
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operator {
    And,
    Or,
    Not,
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
}

/// A condition in the policy AST
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub op: Operator,
    #[serde(default)]
    pub conditions: Vec<Self>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// A complete policy with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub name: String,
    pub description: Option<String>,
    pub condition: Condition,
}

impl Condition {
    #[must_use]
    pub fn and(conditions: Vec<Self>) -> Self {
        Self {
            op: Operator::And,
            conditions,
            field: None,
            value: None,
        }
    }

    #[must_use]
    pub fn or(conditions: Vec<Self>) -> Self {
        Self {
            op: Operator::Or,
            conditions,
            field: None,
            value: None,
        }
    }

    #[must_use]
    pub fn negate(condition: Self) -> Self {
        Self {
            op: Operator::Not,
            conditions: vec![condition],
            field: None,
            value: None,
        }
    }

    #[must_use]
    pub fn eq(field: &str, value: Value) -> Self {
        Self {
            op: Operator::Eq,
            conditions: vec![],
            field: Some(field.to_string()),
            value: Some(value),
        }
    }

    #[must_use]
    pub fn gte(field: &str, value: Value) -> Self {
        Self {
            op: Operator::Gte,
            conditions: vec![],
            field: Some(field.to_string()),
            value: Some(value),
        }
    }

    #[must_use]
    pub fn lte(field: &str, value: Value) -> Self {
        Self {
            op: Operator::Lte,
            conditions: vec![],
            field: Some(field.to_string()),
            value: Some(value),
        }
    }

    #[must_use]
    pub fn in_list(field: &str, values: Vec<Value>) -> Self {
        Self {
            op: Operator::In,
            conditions: vec![],
            field: Some(field.to_string()),
            value: Some(Value::Array(values)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_condition_builders() {
        let cond = Condition::eq("tier", Value::String("verified".to_string()));
        assert_eq!(cond.op, Operator::Eq);
        assert_eq!(cond.field, Some("tier".to_string()));

        let cond = Condition::gte("reputation_score", Value::Number(50.0));
        assert_eq!(cond.op, Operator::Gte);

        let cond = Condition::and(vec![
            Condition::eq("tier", Value::String("verified".to_string())),
            Condition::gte("reputation_score", Value::Number(50.0)),
        ]);
        assert_eq!(cond.op, Operator::And);
        assert_eq!(cond.conditions.len(), 2);
    }

    #[test]
    fn test_json_serialization() -> Result<()> {
        let policy = Policy {
            name: "test_policy".to_string(),
            description: Some("Test policy".to_string()),
            condition: Condition::and(vec![
                Condition::eq("tier", Value::String("verified".to_string())),
                Condition::gte("reputation_score", Value::Number(50.0)),
            ]),
        };

        let json = serde_json::to_string(&policy)?;
        let deserialized: Policy = serde_json::from_str(&json)?;
        assert_eq!(policy.name, deserialized.name);
        Ok(())
    }
}
