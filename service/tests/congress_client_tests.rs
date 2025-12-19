//! Integration tests for CongressApiClient using HTTP stubbing.
//!
//! These tests demonstrate how to use `MockHttpServer` to test HTTP clients
//! against stubbed responses without making real network calls.

mod common;

use common::http_mock::MockHttpServer;
use serde_json::json;
use tinycongress_api::congress::{CongressApiClient, CongressApiError, HttpCongressClient};

/// Test successful member lookup with stubbed HTTP response.
#[tokio::test]
async fn test_get_member_success() {
    // Start mock server
    let server = MockHttpServer::start().await;

    // Stub the API response
    server
        .expect_get("/members/A000360")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_json(json!({
            "member": {
                "id": "A000360",
                "name": "Lamar Alexander",
                "state": "TN",
                "party": "R",
                "chamber": "Senate"
            }
        }))
        .mount()
        .await;

    // Create client pointing to mock server
    let client = HttpCongressClient::new(server.url(), "test-api-key");

    // Make the call
    let member = client.get_member("A000360").await.expect("should succeed");

    // Verify response
    assert_eq!(member.id, "A000360");
    assert_eq!(member.name, "Lamar Alexander");
    assert_eq!(member.state, "TN");
    assert_eq!(member.party, "R");
    assert_eq!(member.chamber, "Senate");
}

/// Test 404 response is handled as NotFound error.
#[tokio::test]
async fn test_get_member_not_found() {
    let server = MockHttpServer::start().await;

    // Stub 404 response
    server
        .expect_get("/members/INVALID")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_status(404)
        .mount()
        .await;

    let client = HttpCongressClient::new(server.url(), "test-api-key");

    let result = client.get_member("INVALID").await;

    assert!(matches!(result, Err(CongressApiError::NotFound(id)) if id == "INVALID"));
}

/// Test API error response (500) is handled correctly.
#[tokio::test]
async fn test_get_member_api_error() {
    let server = MockHttpServer::start().await;

    // Stub 500 error with message
    server
        .expect_get("/members/A000360")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_status(500)
        .with_json_response(json!({"error": "Internal server error"}))
        .mount()
        .await;

    let client = HttpCongressClient::new(server.url(), "test-api-key");

    let result = client.get_member("A000360").await;

    assert!(matches!(
        result,
        Err(CongressApiError::ApiError { status: 500, .. })
    ));
}

/// Test listing members with no filter.
#[tokio::test]
async fn test_list_members_success() {
    let server = MockHttpServer::start().await;

    server
        .expect_get("/members")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_json(json!({
            "members": [
                {
                    "id": "A000360",
                    "name": "Lamar Alexander",
                    "state": "TN",
                    "party": "R",
                    "chamber": "Senate"
                },
                {
                    "id": "B001288",
                    "name": "Cory Booker",
                    "state": "NJ",
                    "party": "D",
                    "chamber": "Senate"
                }
            ]
        }))
        .mount()
        .await;

    let client = HttpCongressClient::new(server.url(), "test-api-key");

    let members = client.list_members(None).await.expect("should succeed");

    assert_eq!(members.len(), 2);
    assert_eq!(members[0].name, "Lamar Alexander");
    assert_eq!(members[1].name, "Cory Booker");
}

/// Test listing members filtered by chamber.
#[tokio::test]
async fn test_list_members_with_chamber_filter() {
    use wiremock::matchers::query_param;
    use wiremock::{Mock, ResponseTemplate};

    let server = MockHttpServer::start().await;

    // Use wiremock directly for query param matching
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/members"))
        .and(query_param("chamber", "Senate"))
        .and(wiremock::matchers::header("X-API-Key", "test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "members": [
                {
                    "id": "A000360",
                    "name": "Lamar Alexander",
                    "state": "TN",
                    "party": "R",
                    "chamber": "Senate"
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = HttpCongressClient::new(server.url(), "test-api-key");

    let members = client
        .list_members(Some("Senate"))
        .await
        .expect("should succeed");

    assert_eq!(members.len(), 1);
    assert_eq!(members[0].chamber, "Senate");
}

/// Test timeout handling using response delay.
#[tokio::test]
async fn test_request_timeout() {
    use std::time::Duration;

    let server = MockHttpServer::start().await;

    // Stub a slow response (5 second delay)
    server
        .expect_get("/members/A000360")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_json(json!({
            "member": {
                "id": "A000360",
                "name": "Lamar Alexander",
                "state": "TN",
                "party": "R",
                "chamber": "Senate"
            }
        }))
        .respond_with_delay(Duration::from_secs(5))
        .mount()
        .await;

    // Create client with short timeout
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .expect("client build");

    let client = HttpCongressClient::with_client(http_client, server.url(), "test-api-key");

    let result = client.get_member("A000360").await;

    // Should fail with request error (timeout)
    assert!(matches!(result, Err(CongressApiError::Request(_))));
}

/// Test call count verification using expect_times.
#[tokio::test]
async fn test_verify_api_called_expected_times() {
    let server = MockHttpServer::start().await;

    server
        .expect_get("/members/A000360")
        .with_header("X-API-Key", "test-api-key")
        .respond_with_json(json!({
            "member": {
                "id": "A000360",
                "name": "Lamar Alexander",
                "state": "TN",
                "party": "R",
                "chamber": "Senate"
            }
        }))
        .expect_times(2) // Expect exactly 2 calls
        .mount()
        .await;

    let client = HttpCongressClient::new(server.url(), "test-api-key");

    // Make 2 calls
    let _ = client.get_member("A000360").await;
    let _ = client.get_member("A000360").await;

    // Verify expectations were met
    server.verify().await;
}

/// Test wrong API key results in no match (404 from mock).
#[tokio::test]
async fn test_wrong_api_key_not_matched() {
    let server = MockHttpServer::start().await;

    // Stub expects specific API key
    server
        .expect_get("/members/A000360")
        .with_header("X-API-Key", "correct-key")
        .respond_with_json(json!({
            "member": {
                "id": "A000360",
                "name": "Lamar Alexander",
                "state": "TN",
                "party": "R",
                "chamber": "Senate"
            }
        }))
        .mount()
        .await;

    // Client uses wrong key
    let client = HttpCongressClient::new(server.url(), "wrong-key");

    let result = client.get_member("A000360").await;

    // Mock returns 404 because header didn't match
    assert!(matches!(result, Err(CongressApiError::NotFound(_))));
}
