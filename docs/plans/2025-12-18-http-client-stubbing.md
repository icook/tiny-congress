# HTTP Client Stubbing Helper Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add declarative HTTP mock server helpers for testing outbound HTTP calls in Rust integration tests.

**Architecture:** Uses `wiremock` crate to provide a fluent API for stubbing external HTTP endpoints. The helper lives in `service/tests/common/http_mock.rs` and follows existing patterns from `MockAccountRepo` (call recording, configurable responses). Tests can declaratively define expected requests and mock responses for success, timeout, and error scenarios.

**Tech Stack:** wiremock 0.6, tokio, serde_json

---

## Task 1: Add wiremock Dependency

**Files:**
- Modify: `service/Cargo.toml:50-58` (dev-dependencies section)

**Step 1: Add wiremock to dev-dependencies**

Open `service/Cargo.toml` and add `wiremock = "0.6"` to the dev-dependencies section:

```toml
[dev-dependencies]
hyper = "1.6"
tower = { version = "0.5", features = ["util"] }
testcontainers = "0.26"
tokio = { version = "1", features = ["sync", "rt-multi-thread"] }
once_cell = "1.19"
tc-test-macros = { path = "../crates/test-macros", version = "0.1.0" }
wiremock = "0.6"
# Enable test-utils feature for integration tests
tinycongress-api = { path = ".", version = "0.1.0", features = ["test-utils"] }
```

**Step 2: Run cargo check to verify dependency resolution**

Run: `cd /Users/icook/tiny-congress/service && cargo check --tests`
Expected: Compiles successfully with wiremock available

**Step 3: Commit**

```bash
git add service/Cargo.toml Cargo.lock
git commit -m "chore: add wiremock dev dependency for HTTP client stubbing"
```

---

## Task 2: Create http_mock Module Structure

**Files:**
- Create: `service/tests/common/http_mock.rs`
- Modify: `service/tests/common/mod.rs:79` (add module export)

**Step 1: Create the http_mock.rs file with module documentation**

Create `service/tests/common/http_mock.rs`:

```rust
//! HTTP mock server helpers for testing outbound HTTP calls.
//!
//! This module provides a thin wrapper around `wiremock` for declarative
//! HTTP stubbing. Use it to mock external API responses in integration tests.
//!
//! # Quick Start
//!
//! ```ignore
//! use crate::common::http_mock::MockServer;
//!
//! #[tokio::test]
//! async fn test_external_api_call() {
//!     let mock = MockServer::start().await;
//!
//!     mock.expect_get("/api/users")
//!         .respond_with_json(json!({"users": []}))
//!         .mount()
//!         .await;
//!
//!     // Your code calls mock.url("/api/users")
//!     // Assertions verify the mock was called
//! }
//! ```
//!
//! # Patterns
//!
//! - **Success response**: `.respond_with_json(value)` or `.respond_with_body(string)`
//! - **Error response**: `.respond_with_status(StatusCode::INTERNAL_SERVER_ERROR)`
//! - **Timeout simulation**: `.respond_with_delay(Duration::from_secs(30))`
//! - **Request verification**: `.expect_times(1)` to assert call count

pub use wiremock::MockServer as WiremockServer;
pub use wiremock::{Mock, ResponseTemplate};
pub use wiremock::matchers::{method, path, body_json, header};
```

**Step 2: Add module export to common/mod.rs**

In `service/tests/common/mod.rs`, add after line 80:

```rust
pub mod http_mock;
```

**Step 3: Run cargo check to verify module compiles**

Run: `cd /Users/icook/tiny-congress/service && cargo check --tests`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add service/tests/common/http_mock.rs service/tests/common/mod.rs
git commit -m "feat: add http_mock module structure with wiremock re-exports"
```

---

## Task 3: Implement MockHttpServer Wrapper

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for MockHttpServer**

Add to `service/tests/common/http_mock.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts_and_returns_url() {
        let server = MockHttpServer::start().await;
        let url = server.url();
        assert!(url.starts_with("http://127.0.0.1:"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_mock_server_starts -v`
Expected: FAIL with "cannot find value `MockHttpServer`"

**Step 3: Implement MockHttpServer struct**

Add to `service/tests/common/http_mock.rs` (before the tests module):

```rust
use std::net::SocketAddr;

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
    pub fn url_for(&self, path: &str) -> String {
        format!("{}{}", self.inner.uri(), path)
    }

    /// Get the socket address the server is listening on.
    pub fn address(&self) -> SocketAddr {
        self.inner.address()
    }

    /// Access the underlying wiremock server for advanced configuration.
    pub fn inner(&self) -> &WiremockServer {
        &self.inner
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_mock_server_starts -v`
Expected: PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: implement MockHttpServer wrapper struct"
```

---

## Task 4: Implement Expectation Builder for GET Requests

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for GET expectation**

Add to the tests module in `service/tests/common/http_mock.rs`:

```rust
    #[tokio::test]
    async fn test_expect_get_returns_json_response() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/test")
            .respond_with_json(serde_json::json!({"status": "ok"}))
            .mount()
            .await;

        // Make actual HTTP request to verify
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
```

**Step 2: Add reqwest dev dependency for testing the mock**

In `service/Cargo.toml`, add to dev-dependencies:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

**Step 3: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_get -v`
Expected: FAIL with "no method named `expect_get`"

**Step 4: Implement ExpectationBuilder and expect_get**

Add to `service/tests/common/http_mock.rs`:

```rust
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Builder for configuring mock expectations.
///
/// Use the fluent API to specify request matching and response behavior.
pub struct ExpectationBuilder<'a> {
    server: &'a MockHttpServer,
    mock: Mock,
}

impl<'a> ExpectationBuilder<'a> {
    fn new(server: &'a MockHttpServer, mock: Mock) -> Self {
        Self { server, mock }
    }

    /// Set the response to return a JSON body with 200 OK status.
    pub fn respond_with_json<T: Serialize>(self, body: T) -> Self {
        let response = ResponseTemplate::new(200)
            .set_body_json(body);
        Self {
            mock: self.mock.respond_with(response),
            ..self
        }
    }

    /// Set a custom response template for full control.
    pub fn respond_with(self, response: ResponseTemplate) -> Self {
        Self {
            mock: self.mock.respond_with(response),
            ..self
        }
    }

    /// Mount this expectation on the server.
    ///
    /// After mounting, any matching requests will receive the configured response.
    pub async fn mount(self) {
        self.mock.mount(self.server.inner()).await;
    }
}

impl MockHttpServer {
    /// Create an expectation for a GET request to the given path.
    pub fn expect_get(&self, request_path: &str) -> ExpectationBuilder<'_> {
        let mock = Mock::given(method("GET")).and(path(request_path));
        ExpectationBuilder::new(self, mock)
    }
}
```

**Step 5: Run test to verify it passes**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_get -v`
Expected: PASS

**Step 6: Commit**

```bash
git add service/tests/common/http_mock.rs service/Cargo.toml Cargo.lock
git commit -m "feat: implement expect_get with JSON response builder"
```

---

## Task 5: Add POST Request Expectations

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for POST expectation**

Add to the tests module:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_post -v`
Expected: FAIL with "no method named `expect_post`"

**Step 3: Implement expect_post and with_json_body**

Add to `MockHttpServer` impl:

```rust
    /// Create an expectation for a POST request to the given path.
    pub fn expect_post(&self, request_path: &str) -> ExpectationBuilder<'_> {
        let mock = Mock::given(method("POST")).and(path(request_path));
        ExpectationBuilder::new(self, mock)
    }
```

Add to `ExpectationBuilder` impl:

```rust
    /// Add a JSON body matcher for the request.
    pub fn with_json_body<T: Serialize>(self, body: T) -> Self {
        use wiremock::matchers::body_json;
        Self {
            mock: self.mock.and(body_json(body)),
            ..self
        }
    }
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_post -v`
Expected: PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: add POST request expectation with body matching"
```

---

## Task 6: Add Error Response Helpers

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for error responses**

Add to the tests module:

```rust
    #[tokio::test]
    async fn test_respond_with_status_returns_error() {
        use wiremock::ResponseTemplate;

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
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_respond_with_status -v`
Expected: FAIL with "no method named `respond_with_status`"

**Step 3: Implement respond_with_status and with_json_response**

Add to `ExpectationBuilder` impl:

```rust
    /// Set the response status code (with empty body).
    pub fn respond_with_status(self, status: u16) -> Self {
        let response = ResponseTemplate::new(status);
        Self {
            mock: self.mock.respond_with(response),
            ..self
        }
    }

    /// Add a JSON body to the response (use after respond_with_status).
    ///
    /// Note: This replaces the current response template. Use for error responses
    /// that need both a status code and body.
    pub fn with_json_response<T: Serialize>(self, body: T) -> Self {
        // Get the current response status if we can infer it, otherwise default to 200
        // For simplicity, we rebuild with the status from respond_with_status
        // This is a builder pattern - the last respond_* call wins
        let response = ResponseTemplate::new(400).set_body_json(body);
        Self {
            mock: self.mock.respond_with(response),
            ..self
        }
    }
```

Wait, that approach is flawed. Let me reconsider. We need to track the status code. Let's update the design:

**Step 3 (revised): Implement with proper state tracking**

Update `ExpectationBuilder` struct:

```rust
pub struct ExpectationBuilder<'a> {
    server: &'a MockHttpServer,
    mock: Mock,
    response: ResponseTemplate,
}

impl<'a> ExpectationBuilder<'a> {
    fn new(server: &'a MockHttpServer, mock: Mock) -> Self {
        Self {
            server,
            mock,
            response: ResponseTemplate::new(200),
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

    /// Add a JSON body matcher for the request.
    pub fn with_json_body<T: Serialize>(self, body: T) -> Self {
        use wiremock::matchers::body_json;
        Self {
            mock: self.mock.and(body_json(body)),
            ..self
        }
    }

    /// Mount this expectation on the server.
    pub async fn mount(self) {
        self.mock
            .respond_with(self.response)
            .mount(self.server.inner())
            .await;
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests -v`
Expected: All PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: add error response helpers with status code support"
```

---

## Task 7: Add Timeout Simulation

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for timeout simulation**

Add to the tests module:

```rust
    #[tokio::test]
    async fn test_respond_with_delay_causes_timeout() {
        use std::time::Duration;

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

        let result = client
            .get(server.url_for("/api/slow"))
            .send()
            .await;

        assert!(result.is_err(), "Expected timeout error");
    }
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_respond_with_delay -v`
Expected: FAIL with "no method named `respond_with_delay`"

**Step 3: Implement respond_with_delay**

Add to `ExpectationBuilder` impl:

```rust
    /// Add a delay before sending the response.
    ///
    /// Use this to simulate slow external services or test timeout handling.
    pub fn respond_with_delay(mut self, delay: std::time::Duration) -> Self {
        self.response = self.response.set_delay(delay);
        self
    }
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_respond_with_delay -v`
Expected: PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: add timeout simulation with respond_with_delay"
```

---

## Task 8: Add Request Verification (expect_times)

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for request verification**

Add to the tests module:

```rust
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
        client.get(server.url_for("/api/counted")).send().await.unwrap();
        client.get(server.url_for("/api/counted")).send().await.unwrap();

        // Verification happens on server drop - this should pass
    }

    #[tokio::test]
    #[should_panic(expected = "Verification failed")]
    async fn test_expect_times_panics_on_wrong_count() {
        let server = MockHttpServer::start().await;

        server
            .expect_get("/api/counted")
            .respond_with_json(serde_json::json!({"count": 1}))
            .expect_times(2)
            .mount()
            .await;

        let client = reqwest::Client::new();

        // Only call once - should panic on drop
        client.get(server.url_for("/api/counted")).send().await.unwrap();

        // Force verification
        server.verify().await;
    }
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_times -v`
Expected: FAIL with "no method named `expect_times`"

**Step 3: Implement expect_times and verify**

Add to `ExpectationBuilder` impl:

```rust
    /// Expect this mock to be called exactly `n` times.
    ///
    /// Verification happens when calling `server.verify()`.
    pub fn expect_times(self, n: u64) -> Self {
        use wiremock::Times;
        Self {
            mock: self.mock.expect(Times::Exactly(n)),
            ..self
        }
    }

    /// Expect this mock to be called at least once.
    pub fn expect_at_least_once(self) -> Self {
        use wiremock::Times;
        Self {
            mock: self.mock.expect(Times::AtLeast(1)),
            ..self
        }
    }
```

Add to `MockHttpServer` impl:

```rust
    /// Verify all mounted expectations were satisfied.
    ///
    /// Panics if any expectation was not met (e.g., wrong call count).
    pub async fn verify(&self) {
        self.inner.verify().await;
    }
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_expect_times -v`
Expected: All PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: add request verification with expect_times"
```

---

## Task 9: Add Header Matching

**Files:**
- Modify: `service/tests/common/http_mock.rs`

**Step 1: Write the failing test for header matching**

Add to the tests module:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_with_header -v`
Expected: FAIL with "no method named `with_header`"

**Step 3: Implement with_header**

Add to `ExpectationBuilder` impl:

```rust
    /// Add a header matcher for the request.
    pub fn with_header(self, name: &str, value: &str) -> Self {
        use wiremock::matchers::header;
        Self {
            mock: self.mock.and(header(name, value)),
            ..self
        }
    }
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock::tests::test_with_header -v`
Expected: All PASS

**Step 5: Commit**

```bash
git add service/tests/common/http_mock.rs
git commit -m "feat: add header matching for request expectations"
```

---

## Task 10: Update Module Documentation with Examples

**Files:**
- Modify: `service/tests/common/http_mock.rs` (module docs)
- Modify: `service/tests/common/mod.rs` (add to module docs)

**Step 1: Update http_mock.rs module documentation**

Replace the module documentation at the top of `service/tests/common/http_mock.rs`:

```rust
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
```

**Step 2: Update common/mod.rs to mention http_mock**

In `service/tests/common/mod.rs`, update the module doc (around line 5-7):

```rust
//! - [`app_builder::TestAppBuilder`] - Build test Axum apps that mirror main.rs wiring
//! - [`test_db`] - Shared PostgreSQL container for database integration tests
//! - [`graphql`] - GraphQL response helpers for testing schema behavior
//! - [`http_mock::MockHttpServer`] - HTTP mock server for testing outbound HTTP calls
```

**Step 3: Run all http_mock tests to ensure nothing broke**

Run: `cd /Users/icook/tiny-congress/service && cargo test --test common http_mock -v`
Expected: All PASS

**Step 4: Commit**

```bash
git add service/tests/common/http_mock.rs service/tests/common/mod.rs
git commit -m "docs: add comprehensive http_mock module documentation"
```

---

## Task 11: Run Full Test Suite and Lint

**Files:** None (validation only)

**Step 1: Run linting**

Run: `just lint-backend`
Expected: All checks pass

**Step 2: Run backend tests**

Run: `just test-backend`
Expected: All tests pass

**Step 3: Run full test suite**

Run: `just test`
Expected: All tests pass

**Step 4: Commit any lint fixes if needed**

If linting required changes:
```bash
git add -A
git commit -m "style: apply lint fixes"
```

---

## Task 12: Final Commit and Summary

**Files:** None

**Step 1: Review all changes**

Run: `git log --oneline origin/master..HEAD`
Expected: See all commits from this implementation

**Step 2: Ensure branch is ready for PR**

Run: `git status`
Expected: Clean working directory

---

## Summary of Files Created/Modified

| File | Action | Purpose |
|------|--------|---------|
| `service/Cargo.toml` | Modified | Add wiremock and reqwest dev dependencies |
| `service/tests/common/mod.rs` | Modified | Export http_mock module, update docs |
| `service/tests/common/http_mock.rs` | Created | HTTP mock server helpers |

## API Reference

### MockHttpServer
- `start()` - Start mock server on random port
- `url()` - Get base URL
- `url_for(path)` - Get URL with path
- `expect_get(path)` - Create GET expectation
- `expect_post(path)` - Create POST expectation
- `verify()` - Verify all expectations met
- `inner()` - Access underlying wiremock server

### ExpectationBuilder
- `respond_with_json(body)` - Return 200 with JSON
- `respond_with_status(code)` - Return status code
- `with_json_response(body)` - Add JSON body to response
- `respond_with_delay(duration)` - Add delay before response
- `with_json_body(body)` - Match request JSON body
- `with_header(name, value)` - Match request header
- `expect_times(n)` - Verify call count
- `expect_at_least_once()` - Verify called at least once
- `respond_with(template)` - Custom ResponseTemplate
- `mount()` - Mount expectation on server
