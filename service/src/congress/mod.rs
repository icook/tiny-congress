//! Congress API client module.
//!
//! Provides HTTP client abstraction for fetching Congress member data
//! from external APIs.
//!
//! # Architecture
//!
//! The module uses a trait-based design for testability:
//!
//! - [`CongressApiClient`] - Trait defining API operations
//! - [`HttpCongressClient`] - Real HTTP implementation using reqwest
//! - [`mock::MockCongressClient`] - Mock for unit tests (behind `test-utils` feature)
//!
//! # Testing Patterns
//!
//! ## Unit Tests (Mock Implementation)
//!
//! Use `MockCongressClient` for fast, isolated unit tests:
//!
//! ```ignore
//! use tinycongress_api::congress::mock::MockCongressClient;
//!
//! let mock = MockCongressClient::new();
//! mock.set_get_member_result(Ok(Member { ... }));
//!
//! // Pass mock to code under test
//! let result = my_service.lookup_member(&mock, "A000360").await;
//! assert!(result.is_ok());
//! ```
//!
//! ## Integration Tests (HTTP Stubbing)
//!
//! Use `MockHttpServer` to test `HttpCongressClient` against stubbed HTTP:
//!
//! ```ignore
//! use crate::common::http_mock::MockHttpServer;
//! use tinycongress_api::congress::HttpCongressClient;
//!
//! let server = MockHttpServer::start().await;
//!
//! server
//!     .expect_get("/members/A000360")
//!     .with_header("X-API-Key", "test-key")
//!     .respond_with_json(json!({
//!         "member": { "id": "A000360", "name": "Lamar Alexander", ... }
//!     }))
//!     .mount()
//!     .await;
//!
//! let client = HttpCongressClient::new(server.url(), "test-key");
//! let member = client.get_member("A000360").await.unwrap();
//! assert_eq!(member.name, "Lamar Alexander");
//! ```

mod client;
mod types;

pub use client::{CongressApiClient, CongressApiError, HttpCongressClient};
pub use types::{Member, MemberResponse, MembersResponse};

#[cfg(any(test, feature = "test-utils"))]
pub use client::mock;
