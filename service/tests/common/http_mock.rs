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

pub use wiremock::matchers::{body_json, header, method, path};
pub use wiremock::MockServer as WiremockServer;
pub use wiremock::{Mock, ResponseTemplate};
