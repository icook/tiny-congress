//! HTTP mock server helpers for testing outbound HTTP calls.
//!
//! This module provides a thin wrapper around `wiremock` for declarative
//! HTTP stubbing. Use it to mock external API responses in integration tests.
//!
//! # Quick Start
//!
//! ```ignore
//! use crate::common::http_mock::MockHttpServer;
//!
//! #[tokio::test]
//! async fn test_external_api_call() {
//!     let server = MockHttpServer::start().await;
//!
//!     server
//!         .expect_get("/api/users")
//!         .respond_with_json(json!({"users": []}))
//!         .mount()
//!         .await;
//!
//!     // Your code calls server.url_for("/api/users")
//!     // Assertions verify the mock was called
//! }
//! ```
//!
//! # Response Patterns
//!
//! ## Success Response
//! ```ignore
//! server.expect_get("/api/data")
//!     .respond_with_json(json!({"key": "value"}))
//!     .mount().await;
//! ```
//!
//! ## Error Response
//! ```ignore
//! server.expect_get("/api/fail")
//!     .respond_with_status(500)
//!     .with_json_response(json!({"error": "internal server error"}))
//!     .mount().await;
//! ```
//!
//! ## Timeout Simulation
//! ```ignore
//! use std::time::Duration;
//!
//! server.expect_get("/api/slow")
//!     .respond_with_delay(Duration::from_secs(30))
//!     .mount().await;
//! ```
//!
//! # Request Matching
//!
//! ## POST with JSON Body
//! ```ignore
//! server.expect_post("/api/create")
//!     .with_json_body(json!({"name": "test"}))
//!     .respond_with_json(json!({"id": 1}))
//!     .mount().await;
//! ```
//!
//! ## With Headers
//! ```ignore
//! server.expect_get("/api/protected")
//!     .with_header("Authorization", "Bearer token")
//!     .respond_with_json(json!({"data": "secret"}))
//!     .mount().await;
//! ```
//!
//! # Request Verification
//!
//! ```ignore
//! server.expect_get("/api/counted")
//!     .respond_with_json(json!({}))
//!     .expect_times(2)
//!     .mount().await;
//!
//! // Make requests...
//!
//! server.verify().await; // Panics if not called exactly 2 times
//! ```
//!
//! # Advanced: Access Underlying wiremock
//!
//! For cases not covered by the fluent API, access the underlying server:
//!
//! ```ignore
//! use wiremock::{Mock, ResponseTemplate};
//! use wiremock::matchers::{method, path, query_param};
//!
//! Mock::given(method("GET"))
//!     .and(path("/search"))
//!     .and(query_param("q", "rust"))
//!     .respond_with(ResponseTemplate::new(200))
//!     .mount(server.inner())
//!     .await;
//! ```

use serde::Serialize;
use std::net::SocketAddr;
use std::time::Duration;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockBuilder, MockServer as WiremockServer, ResponseTemplate};

/// A wrapper around wiremock's MockServer providing a simplified API.
///
/// This provides a fluent builder pattern for common mocking scenarios
/// while still allowing access to the underlying wiremock server for
/// advanced use cases.
pub struct MockHttpServer {
    inner: WiremockServer,
}

impl MockHttpServer {
    /// Start a new mock server on a random available port.
    pub async fn start() -> Self {
        let inner = WiremockServer::start().await;
        Self { inner }
    }

    /// Get the base URL of the mock server (e.g., "http://127.0.0.1:12345").
    pub fn url(&self) -> String {
        self.inner.uri()
    }

    /// Get the base URL with a path appended.
    pub fn url_for(&self, request_path: &str) -> String {
        format!("{}{}", self.inner.uri(), request_path)
    }

    /// Get the socket address the server is listening on.
    pub fn address(&self) -> SocketAddr {
        *self.inner.address()
    }

    /// Access the underlying wiremock server for advanced configuration.
    pub fn inner(&self) -> &WiremockServer {
        &self.inner
    }

    /// Verify all mounted expectations were satisfied.
    ///
    /// Panics if any expectation was not met (e.g., wrong call count).
    pub async fn verify(&self) {
        self.inner.verify().await;
    }

    /// Create an expectation for a GET request to the given path.
    pub fn expect_get(&self, request_path: &str) -> ExpectationBuilder<'_> {
        let builder = Mock::given(method("GET")).and(path(request_path));
        ExpectationBuilder::new(self, builder)
    }

    /// Create an expectation for a POST request to the given path.
    pub fn expect_post(&self, request_path: &str) -> ExpectationBuilder<'_> {
        let builder = Mock::given(method("POST")).and(path(request_path));
        ExpectationBuilder::new(self, builder)
    }
}

/// Builder for configuring mock expectations.
///
/// Use the fluent API to specify request matching and response behavior.
pub struct ExpectationBuilder<'a> {
    server: &'a MockHttpServer,
    builder: MockBuilder,
    response: ResponseTemplate,
    times: Option<u64>,
    at_least_once: bool,
}

impl<'a> ExpectationBuilder<'a> {
    fn new(server: &'a MockHttpServer, builder: MockBuilder) -> Self {
        Self {
            server,
            builder,
            response: ResponseTemplate::new(200),
            times: None,
            at_least_once: false,
        }
    }

    /// Set the response to return a JSON body with 200 OK status.
    pub fn respond_with_json<T: Serialize>(mut self, body: T) -> Self {
        self.response = ResponseTemplate::new(200).set_body_json(body);
        self
    }

    /// Set the response status code.
    pub fn respond_with_status(mut self, status: u16) -> Self {
        self.response = ResponseTemplate::new(status);
        self
    }

    /// Add a JSON body to the current response.
    pub fn with_json_response<T: Serialize>(mut self, body: T) -> Self {
        self.response = self.response.set_body_json(body);
        self
    }

    /// Set a custom response template for full control.
    pub fn respond_with(mut self, response: ResponseTemplate) -> Self {
        self.response = response;
        self
    }

    /// Add a delay before sending the response.
    ///
    /// Use this to simulate slow external services or test timeout handling.
    pub fn respond_with_delay(mut self, delay: Duration) -> Self {
        self.response = self.response.set_delay(delay);
        self
    }

    /// Add a JSON body matcher for the request.
    pub fn with_json_body<T: Serialize>(mut self, body: T) -> Self {
        self.builder = self.builder.and(body_json(body));
        self
    }

    /// Add a header matcher for the request.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.builder = self.builder.and(header(name, value));
        self
    }

    /// Expect this mock to be called exactly `n` times.
    ///
    /// Verification happens when calling `server.verify()`.
    pub fn expect_times(mut self, n: u64) -> Self {
        self.times = Some(n);
        self
    }

    /// Expect this mock to be called at least once.
    pub fn expect_at_least_once(mut self) -> Self {
        self.at_least_once = true;
        self
    }

    /// Mount this expectation on the server.
    ///
    /// After mounting, any matching requests will receive the configured response.
    pub async fn mount(self) {
        let mock = self.builder.respond_with(self.response);

        // Apply expectations if set
        let mock = if let Some(n) = self.times {
            mock.expect(n)
        } else if self.at_least_once {
            mock.expect(1..)
        } else {
            mock
        };

        mock.mount(self.server.inner()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts_and_returns_url() {
        let server = MockHttpServer::start().await;
        let url = server.url();
        assert!(url.starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn test_expect_get_returns_json_response() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/test")
            .respond_with_json(serde_json::json!({"status": "ok"}))
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(server.url_for("/api/test"))
            .send()
            .await
            .expect("request failed");

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.expect("json parse failed");
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn test_expect_post_matches_body() {
        let server = MockHttpServer::start().await;

        server
            .expect_post("/api/create")
            .with_json_body(serde_json::json!({"name": "test"}))
            .respond_with_json(serde_json::json!({"id": 1, "name": "test"}))
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(server.url_for("/api/create"))
            .json(&serde_json::json!({"name": "test"}))
            .send()
            .await
            .expect("request failed");

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.expect("json parse failed");
        assert_eq!(body["id"], 1);
    }

    #[tokio::test]
    async fn test_respond_with_status_returns_error() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/fail")
            .respond_with_status(500)
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(server.url_for("/api/fail"))
            .send()
            .await
            .expect("request failed");

        assert_eq!(response.status(), 500);
    }

    #[tokio::test]
    async fn test_respond_with_status_and_json_body() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/error")
            .respond_with_status(400)
            .with_json_response(serde_json::json!({"error": "bad request"}))
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(server.url_for("/api/error"))
            .send()
            .await
            .expect("request failed");

        assert_eq!(response.status(), 400);
        let body: serde_json::Value = response.json().await.expect("json parse failed");
        assert_eq!(body["error"], "bad request");
    }

    #[tokio::test]
    async fn test_respond_with_delay_causes_timeout() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/slow")
            .respond_with_delay(Duration::from_secs(5))
            .mount()
            .await;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .expect("client build failed");

        let result = client.get(server.url_for("/api/slow")).send().await;

        assert!(result.is_err(), "Expected timeout error");
    }

    #[tokio::test]
    async fn test_expect_times_verifies_call_count() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/counted")
            .respond_with_json(serde_json::json!({"count": 1}))
            .expect_times(2)
            .mount()
            .await;

        let client = reqwest::Client::new();

        // Call exactly twice
        client
            .get(server.url_for("/api/counted"))
            .send()
            .await
            .unwrap();
        client
            .get(server.url_for("/api/counted"))
            .send()
            .await
            .unwrap();

        // Verification should pass
        server.verify().await;
    }

    #[tokio::test]
    async fn test_with_header_matches_request_header() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/auth")
            .with_header("Authorization", "Bearer token123")
            .respond_with_json(serde_json::json!({"authenticated": true}))
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(server.url_for("/api/auth"))
            .header("Authorization", "Bearer token123")
            .send()
            .await
            .expect("request failed");

        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn test_with_header_rejects_wrong_header() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/auth")
            .with_header("Authorization", "Bearer token123")
            .respond_with_json(serde_json::json!({"authenticated": true}))
            .mount()
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(server.url_for("/api/auth"))
            .header("Authorization", "Bearer wrong")
            .send()
            .await
            .expect("request failed");

        // Should get 404 because header didn't match
        assert_eq!(response.status(), 404);
    }
}
