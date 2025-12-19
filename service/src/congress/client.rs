//! Congress API client for fetching member data.
//!
//! This module provides a trait-based HTTP client for interacting with
//! external Congress data APIs. The trait abstraction enables:
//!
//! - Easy mocking in unit tests
//! - HTTP-level testing with `MockHttpServer` in integration tests
//! - Swapping implementations (e.g., different API providers)
//!
//! # Example
//!
//! ```ignore
//! use tinycongress_api::congress::{CongressApiClient, HttpCongressClient};
//!
//! let client = HttpCongressClient::new("https://api.congress.gov", "my-api-key");
//! let member = client.get_member("A000360").await?;
//! println!("Found: {} from {}", member.name, member.state);
//! ```

use async_trait::async_trait;
use thiserror::Error;

use super::types::{Member, MemberResponse, MembersResponse};

/// Errors that can occur when calling the Congress API.
#[derive(Debug, Error)]
pub enum CongressApiError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// Member not found
    #[error("Member not found: {0}")]
    NotFound(String),

    /// API returned an error response
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
}

/// Trait for Congress API operations.
///
/// Implementations can fetch member data from external APIs.
/// Use `HttpCongressClient` for real HTTP calls, or create a mock
/// implementation for testing.
#[async_trait]
pub trait CongressApiClient: Send + Sync {
    /// Get a single member by their Bioguide ID.
    async fn get_member(&self, id: &str) -> Result<Member, CongressApiError>;

    /// List all members, optionally filtered by chamber.
    async fn list_members(&self, chamber: Option<&str>) -> Result<Vec<Member>, CongressApiError>;
}

/// HTTP-based implementation of `CongressApiClient`.
///
/// Makes real HTTP requests to an external Congress API.
pub struct HttpCongressClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl HttpCongressClient {
    /// Create a new client with the given base URL and API key.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// Create a client with a custom `reqwest::Client` (for testing with custom config).
    pub fn with_client(
        client: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl CongressApiClient for HttpCongressClient {
    async fn get_member(&self, id: &str) -> Result<Member, CongressApiError> {
        let url = format!("{}/members/{}", self.base_url, id);

        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(CongressApiError::NotFound(id.to_string()));
        }

        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(CongressApiError::ApiError {
                status: status.as_u16(),
                message,
            });
        }

        let member_response: MemberResponse = response.json().await?;
        Ok(member_response.member)
    }

    async fn list_members(&self, chamber: Option<&str>) -> Result<Vec<Member>, CongressApiError> {
        let mut url = format!("{}/members", self.base_url);

        if let Some(c) = chamber {
            url = format!("{url}?chamber={c}");
        }

        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(CongressApiError::ApiError {
                status: status.as_u16(),
                message,
            });
        }

        let members_response: MembersResponse = response.json().await?;
        Ok(members_response.members)
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate
)]
pub mod mock {
    //! Mock implementation for unit testing.

    use super::{CongressApiClient, CongressApiError, Member};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock implementation of `CongressApiClient` for unit tests.
    ///
    /// Configure responses with `set_*_result` methods and verify
    /// calls with `get_member_calls()` and `list_members_calls()`.
    pub struct MockCongressClient {
        get_member_result: Mutex<Option<Result<Member, CongressApiError>>>,
        list_members_result: Mutex<Option<Result<Vec<Member>, CongressApiError>>>,
        get_member_calls: Mutex<Vec<String>>,
        list_members_calls: Mutex<Vec<Option<String>>>,
    }

    impl MockCongressClient {
        pub fn new() -> Self {
            Self {
                get_member_result: Mutex::new(None),
                list_members_result: Mutex::new(None),
                get_member_calls: Mutex::new(Vec::new()),
                list_members_calls: Mutex::new(Vec::new()),
            }
        }

        /// Set the result for `get_member` calls.
        pub fn set_get_member_result(&self, result: Result<Member, CongressApiError>) {
            *self.get_member_result.lock().unwrap() = Some(result);
        }

        /// Set the result for `list_members` calls.
        pub fn set_list_members_result(&self, result: Result<Vec<Member>, CongressApiError>) {
            *self.list_members_result.lock().unwrap() = Some(result);
        }

        /// Get all IDs passed to `get_member`.
        pub fn get_member_calls(&self) -> Vec<String> {
            self.get_member_calls.lock().unwrap().clone()
        }

        /// Get all chamber filters passed to `list_members`.
        pub fn list_members_calls(&self) -> Vec<Option<String>> {
            self.list_members_calls.lock().unwrap().clone()
        }
    }

    impl Default for MockCongressClient {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl CongressApiClient for MockCongressClient {
        async fn get_member(&self, id: &str) -> Result<Member, CongressApiError> {
            self.get_member_calls.lock().unwrap().push(id.to_string());

            self.get_member_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Err(CongressApiError::NotFound(id.to_string())))
        }

        async fn list_members(
            &self,
            chamber: Option<&str>,
        ) -> Result<Vec<Member>, CongressApiError> {
            self.list_members_calls
                .lock()
                .unwrap()
                .push(chamber.map(String::from));

            self.list_members_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
    }
}
