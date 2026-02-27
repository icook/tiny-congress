//! GraphQL response helpers for integration tests.
//!
//! This module provides utilities for executing GraphQL queries/mutations
//! and asserting on their responses.

use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::build_info::BuildInfo;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

/// Execute a GraphQL query against the test schema and return parsed JSON.
pub async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfo::from_env())
        .finish();
    let response = schema.execute(query).await;
    serde_json::to_value(response).expect("Failed to serialize GraphQL response")
}

/// Helper to extract the "data" field from a GraphQL response.
pub fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data").filter(|v| !v.is_null())
}

/// Helper to extract errors from a GraphQL response.
pub fn extract_errors(response: &Value) -> &[Value] {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

/// Assert that a GraphQL response has no errors.
pub fn assert_no_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        errors.is_empty(),
        "Expected no GraphQL errors, but got: {:?}",
        errors
    );
}

/// Assert that a GraphQL response contains at least one error.
pub fn assert_has_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        !errors.is_empty(),
        "Expected GraphQL errors, but response had none"
    );
}
