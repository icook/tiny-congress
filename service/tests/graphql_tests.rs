//! Direct GraphQL schema execution tests.
//!
//! These tests verify GraphQL query/mutation behavior by directly executing
//! against the schema, without going through HTTP. This allows testing
//! schema-level behavior in isolation.

use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::build_info::BuildInfoProvider;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

// ============================================================================
// Test Helpers
// ============================================================================

/// Execute a GraphQL query against the test schema and return parsed JSON.
async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfoProvider::from_env())
        .finish();
    let response = schema.execute(query).await;
    serde_json::to_value(response).expect("Failed to serialize GraphQL response")
}

/// Helper to extract the "data" field from a GraphQL response.
fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data").filter(|v| !v.is_null())
}

/// Helper to extract errors from a GraphQL response.
fn extract_errors(response: &Value) -> &[Value] {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

/// Assert that a GraphQL response has no errors.
fn assert_no_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        errors.is_empty(),
        "Expected no GraphQL errors, but got: {:?}",
        errors
    );
}

/// Assert that a GraphQL response contains at least one error.
fn assert_has_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        !errors.is_empty(),
        "Expected GraphQL errors, but response had none"
    );
}

// ============================================================================
// Basic Query/Mutation Tests
// ============================================================================

#[tokio::test]
async fn test_build_info_query() {
    let query = r#"
        {
            buildInfo {
                version
                gitSha
                buildTime
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Response should have data");
    let build_info = &data["buildInfo"];

    // Assert structure - these will fail on schema drift
    assert!(build_info.is_object(), "buildInfo should be an object");
    assert!(
        build_info["version"].is_string(),
        "version should be a string"
    );
    assert!(
        build_info["gitSha"].is_string(),
        "gitSha should be a string"
    );
    assert!(
        build_info["buildTime"].is_string(),
        "buildTime should be a string"
    );

    // Assert values are non-empty
    assert!(
        !build_info["version"]
            .as_str()
            .expect("version should be string")
            .is_empty(),
        "version should not be empty"
    );
    assert!(
        !build_info["gitSha"]
            .as_str()
            .expect("gitSha should be string")
            .is_empty(),
        "gitSha should not be empty"
    );
    assert!(
        !build_info["buildTime"]
            .as_str()
            .expect("buildTime should be string")
            .is_empty(),
        "buildTime should not be empty"
    );
}

#[tokio::test]
async fn test_echo_mutation() {
    let mutation = r#"
        mutation {
            echo(message: "hello world")
        }
    "#;

    let result = execute_query(mutation).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Response should have data");
    let echo = &data["echo"];

    assert!(echo.is_string(), "echo should return a string");
    assert_eq!(
        echo.as_str().expect("echo should be string"),
        "hello world",
        "echo should return the exact input message"
    );
}

#[tokio::test]
async fn test_echo_mutation_empty_string() {
    let mutation = r#"
        mutation {
            echo(message: "")
        }
    "#;

    let result = execute_query(mutation).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Response should have data");
    let echo = &data["echo"];

    assert_eq!(
        echo.as_str().expect("echo should be string"),
        "",
        "echo should handle empty strings"
    );
}

#[tokio::test]
async fn test_echo_mutation_special_characters() {
    let mutation = r#"
        mutation {
            echo(message: "Hello \"world\"!\nLine 2\t\u0000")
        }
    "#;

    let result = execute_query(mutation).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Response should have data");
    let echo = &data["echo"];

    assert!(echo.is_string(), "echo should handle special characters");
    // The exact string content depends on GraphQL string parsing
    assert!(
        echo.as_str()
            .expect("echo should be string")
            .contains("Hello"),
        "echo should preserve message content"
    );
}

// ============================================================================
// Error Test Cases
// ============================================================================

#[tokio::test]
async fn test_invalid_query_syntax() {
    let query = r#"{ buildInfo { version "#; // Missing closing braces

    let result = execute_query(query).await;
    assert_has_errors(&result);

    // Data should be null or missing when query is syntactically invalid
    assert!(
        extract_data(&result).is_none(),
        "Syntactically invalid queries should not return data"
    );
}

#[tokio::test]
async fn test_unknown_field() {
    let query = r#"
        {
            buildInfo {
                version
                unknownField
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_has_errors(&result);

    // Verify error message mentions the unknown field
    let errors = &result["errors"];
    let error_message = errors[0]["message"]
        .as_str()
        .expect("Error should have message");
    assert!(
        error_message.contains("unknownField") || error_message.contains("Unknown field"),
        "Error message should mention the unknown field: {}",
        error_message
    );
}

#[tokio::test]
async fn test_unknown_query_root_field() {
    let query = r#"
        {
            nonExistentQuery {
                field
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_has_errors(&result);
}

#[tokio::test]
async fn test_mutation_missing_required_argument() {
    let mutation = r#"
        mutation {
            echo
        }
    "#;

    let result = execute_query(mutation).await;
    assert_has_errors(&result);

    // Verify error mentions missing argument
    let errors = &result["errors"];
    let error_message = errors[0]["message"]
        .as_str()
        .expect("Error should have message");
    assert!(
        error_message.contains("message") || error_message.contains("argument"),
        "Error should mention missing 'message' argument: {}",
        error_message
    );
}

#[tokio::test]
async fn test_mutation_wrong_argument_type() {
    let mutation = r#"
        mutation {
            echo(message: 123)
        }
    "#;

    let result = execute_query(mutation).await;
    assert_has_errors(&result);
}

#[tokio::test]
async fn test_query_type_mismatch() {
    // Try to use mutation syntax for a query field
    let query = r#"
        mutation {
            buildInfo {
                version
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_has_errors(&result);
}

#[tokio::test]
async fn test_empty_query() {
    let query = "";

    let result = execute_query(query).await;
    assert_has_errors(&result);
}

#[tokio::test]
async fn test_whitespace_only_query() {
    let query = "   \n\t   ";

    let result = execute_query(query).await;
    assert_has_errors(&result);
}

// ============================================================================
// Schema Introspection Tests
// ============================================================================

#[tokio::test]
async fn test_schema_introspection_type() {
    let query = r#"
        {
            __schema {
                queryType {
                    name
                }
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Introspection should return data");
    let query_type_name = &data["__schema"]["queryType"]["name"];
    assert_eq!(
        query_type_name.as_str().expect("name should be string"),
        "QueryRoot",
        "Query type should be QueryRoot"
    );
}

#[tokio::test]
async fn test_build_info_type_introspection() {
    let query = r#"
        {
            __type(name: "BuildInfo") {
                name
                fields {
                    name
                    type {
                        name
                        kind
                    }
                }
            }
        }
    "#;

    let result = execute_query(query).await;
    assert_no_errors(&result);

    let data = extract_data(&result).expect("Type introspection should return data");
    let type_info = &data["__type"];

    assert_eq!(
        type_info["name"].as_str().expect("name should be string"),
        "BuildInfo"
    );

    let fields = type_info["fields"]
        .as_array()
        .expect("fields should be an array");

    // Verify expected fields exist
    let field_names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();

    assert!(
        field_names.contains(&"version"),
        "BuildInfo should have version field"
    );
    assert!(
        field_names.contains(&"gitSha"),
        "BuildInfo should have gitSha field"
    );
    assert!(
        field_names.contains(&"buildTime"),
        "BuildInfo should have buildTime field"
    );
}
