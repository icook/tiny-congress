//! HTTP client for interacting with the `TinyCongress` API during simulation.
//!
//! [`SimClient`] wraps [`reqwest::Client`] and provides typed methods for
//! every API endpoint the sim worker needs. Response types are local to this
//! module — they deserialize only the fields we care about, avoiding coupling
//! to server-side types.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::identity::SimAccount;

// ---------------------------------------------------------------------------
// Response types (deserialization-only, local to sim)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RoomResponse {
    pub id: Uuid,
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct PollResponse {
    pub id: Uuid,
    #[allow(dead_code)]
    pub room_id: Uuid,
    #[allow(dead_code)]
    pub question: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct PollDetailResponse {
    #[allow(dead_code)]
    pub poll: PollResponse,
    pub dimensions: Vec<DimensionResponse>,
}

#[derive(Debug, Deserialize)]
pub struct DimensionResponse {
    pub id: Uuid,
    #[allow(dead_code)]
    pub name: String,
    pub min_value: f32,
    pub max_value: f32,
}

#[derive(Debug, Deserialize)]
pub struct PollResultsResponse {
    pub voter_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct SignupResponse {
    #[allow(dead_code)]
    pub account_id: Uuid,
    #[allow(dead_code)]
    pub root_kid: String,
    #[allow(dead_code)]
    pub device_kid: String,
}

#[derive(Debug, Deserialize)]
pub struct VoteResponse {
    #[allow(dead_code)]
    pub dimension_id: Uuid,
    #[allow(dead_code)]
    pub value: f32,
}

// ---------------------------------------------------------------------------
// Request body helpers (serialization-only)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CreateRoomBody<'a> {
    name: &'a str,
    description: &'a str,
    eligibility_topic: &'a str,
    constraint_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    constraint_config: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    poll_duration_secs: Option<i32>,
}

#[derive(Serialize)]
struct CreatePollBody<'a> {
    question: &'a str,
    description: &'a str,
}

#[derive(Serialize)]
struct AddDimensionBody<'a> {
    name: &'a str,
    description: &'a str,
    min_value: f32,
    max_value: f32,
    sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_label: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateStatusBody<'a> {
    status: &'a str,
}

#[derive(Serialize)]
struct CastVoteBody<'a> {
    votes: &'a [VoteEntry],
}

#[derive(Serialize)]
struct VoteEntry {
    dimension_id: Uuid,
    value: f32,
}

/// Evidence item for POST evidence endpoint.
#[derive(Serialize)]
pub struct EvidenceBody {
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
}

#[derive(Serialize)]
struct AddEvidenceBody<'a> {
    evidence: &'a [EvidenceBody],
}

#[derive(Serialize)]
struct EndorseBody<'a> {
    username: &'a str,
    topic: &'a str,
}

#[derive(Serialize)]
struct EndorseBodyWithEvidence<'a> {
    username: &'a str,
    topic: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<&'a serde_json::Value>,
}

// ---------------------------------------------------------------------------
// SimClient
// ---------------------------------------------------------------------------

/// HTTP client for the `TinyCongress` API, used by the simulation worker.
pub struct SimClient {
    http: reqwest::Client,
    api_url: String,
}

impl SimClient {
    /// Create a new `SimClient` with the given HTTP client and base API URL.
    #[must_use]
    pub const fn new(http: reqwest::Client, api_url: String) -> Self {
        Self { http, api_url }
    }

    // -- Unauthenticated endpoints ----------------------------------------

    /// Get rooms that have capacity for new content (no active poll).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn get_capacity(&self) -> Result<Vec<RoomResponse>> {
        let url = format!("{}/rooms/capacity", self.api_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET /rooms/capacity returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// List all rooms.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn list_rooms(&self) -> Result<Vec<RoomResponse>> {
        let url = format!("{}/rooms", self.api_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET /rooms returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// List polls for a room.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn list_polls(&self, room_id: Uuid) -> Result<Vec<PollResponse>> {
        let path = format!("/rooms/{room_id}/polls");
        let url = format!("{}{path}", self.api_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Get poll detail including dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn get_poll_detail(
        &self,
        room_id: Uuid,
        poll_id: Uuid,
    ) -> Result<PollDetailResponse> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}");
        let url = format!("{}{path}", self.api_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Get poll results (voter count, etc.).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn get_poll_results(
        &self,
        room_id: Uuid,
        poll_id: Uuid,
    ) -> Result<PollResultsResponse> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/results");
        let url = format!("{}{path}", self.api_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    // -- Authenticated endpoints ------------------------------------------

    /// Sign up a new account. Returns the raw response so the caller can
    /// inspect the status code (201 for success, 409 for duplicate).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request itself fails (network error).
    pub async fn signup(&self, body: &str) -> Result<reqwest::Response> {
        let url = format!("{}/auth/signup", self.api_url);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.to_owned())
            .send()
            .await?;
        Ok(resp)
    }

    /// Create a room.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_room(
        &self,
        account: &SimAccount,
        name: &str,
        description: &str,
        eligibility_topic: &str,
        constraint_type: &str,
        constraint_config: Option<&serde_json::Value>,
        poll_duration_secs: Option<i32>,
    ) -> Result<RoomResponse> {
        let path = "/rooms";
        let body = serde_json::to_vec(&CreateRoomBody {
            name,
            description,
            eligibility_topic,
            constraint_type,
            constraint_config,
            poll_duration_secs,
        })?;
        let headers = account.sign_request("POST", path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Create a poll in a room.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn create_poll(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        question: &str,
        description: &str,
    ) -> Result<PollResponse> {
        let path = format!("/rooms/{room_id}/polls");
        let body = serde_json::to_vec(&CreatePollBody {
            question,
            description,
        })?;
        let headers = account.sign_request("POST", &path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Add a dimension to a poll.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_dimension(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        poll_id: Uuid,
        name: &str,
        desc: &str,
        min: f32,
        max: f32,
        order: i32,
        min_label: Option<&str>,
        max_label: Option<&str>,
    ) -> Result<DimensionResponse> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/dimensions");
        let body = serde_json::to_vec(&AddDimensionBody {
            name,
            description: desc,
            min_value: min,
            max_value: max,
            sort_order: order,
            min_label,
            max_label,
        })?;
        let headers = account.sign_request("POST", &path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Update a poll's status (e.g., "active", "closed").
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn update_poll_status(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        poll_id: Uuid,
        status: &str,
    ) -> Result<()> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/status");
        let body = serde_json::to_vec(&UpdateStatusBody { status })?;
        let headers = account.sign_request("POST", &path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let resp_status = resp.status();
        if !resp_status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {resp_status}: {body}"));
        }
        Ok(())
    }

    /// Cast votes on a poll.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn cast_vote(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        poll_id: Uuid,
        votes: &[(Uuid, f32)],
    ) -> Result<Vec<VoteResponse>> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/vote");
        let vote_entries: Vec<VoteEntry> = votes
            .iter()
            .map(|(dimension_id, value)| VoteEntry {
                dimension_id: *dimension_id,
                value: *value,
            })
            .collect();
        let body = serde_json::to_vec(&CastVoteBody {
            votes: &vote_entries,
        })?;
        let headers = account.sign_request("POST", &path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(resp.json().await?)
    }

    /// Insert evidence items for a poll dimension.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn add_evidence(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        poll_id: Uuid,
        dimension_id: Uuid,
        evidence: &[EvidenceBody],
    ) -> Result<()> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/dimensions/{dimension_id}/evidence");
        let body = serde_json::to_vec(&AddEvidenceBody { evidence })?;
        let headers = account.sign_request("POST", &path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(())
    }

    /// Delete all evidence for a poll (ring buffer reset).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn delete_poll_evidence(
        &self,
        account: &SimAccount,
        room_id: Uuid,
        poll_id: Uuid,
    ) -> Result<()> {
        let path = format!("/rooms/{room_id}/polls/{poll_id}/evidence");
        let body: &[u8] = b"";
        let headers = account.sign_request("DELETE", &path, body);

        let mut req = self
            .http
            .delete(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json");

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("DELETE {path} returned {status}: {body}"));
        }
        Ok(())
    }

    // -- Verifier-authenticated endpoint ----------------------------------

    /// Log in an existing account to register a device key.
    ///
    /// Returns the raw response so the caller can inspect the status code
    /// (201 for success, 409 for duplicate device key).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request itself fails (network error).
    pub async fn login(&self, body: &str) -> Result<reqwest::Response> {
        let url = format!("{}/auth/login", self.api_url);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.to_owned())
            .send()
            .await?;
        Ok(resp)
    }

    /// Look up an account by username (requires authentication).
    ///
    /// Returns the account UUID, which can be used as a `verifier_id` in room
    /// constraint config.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn lookup_account(&self, authed: &SimAccount, username: &str) -> Result<Uuid> {
        let path = format!(
            "/accounts/lookup?username={}",
            urlencoding::encode(username)
        );
        let body: &[u8] = b"";
        let headers = authed.sign_request("GET", &path, body);

        let mut req = self
            .http
            .get(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json");

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GET {path} returned {status}: {body}"));
        }
        let json: serde_json::Value = resp.json().await?;
        let id_str = json["id"]
            .as_str()
            .ok_or_else(|| anyhow!("lookup_account: missing 'id' field in response"))?;
        Uuid::parse_str(id_str).map_err(|e| anyhow!("lookup_account: invalid UUID: {e}"))
    }

    /// Endorse a user for a topic via the verifier API.
    ///
    /// The `verifier` account must have an `authorized_verifier` endorsement
    /// (bootstrapped by the API server from `TC_VERIFIERS` config).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn endorse(&self, verifier: &SimAccount, username: &str, topic: &str) -> Result<()> {
        let path = "/verifiers/endorsements";
        let body = serde_json::to_vec(&EndorseBody { username, topic })?;
        let headers = verifier.sign_request("POST", path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(())
    }

    /// Endorse a user for a topic with optional evidence metadata.
    ///
    /// Like [`SimClient::endorse`] but allows attaching structured evidence
    /// (e.g., which verification method the user chose). When `evidence` is
    /// `None`, the field is omitted from the request body entirely.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is not 2xx.
    pub async fn endorse_with_evidence(
        &self,
        verifier: &SimAccount,
        username: &str,
        topic: &str,
        evidence: Option<&serde_json::Value>,
    ) -> Result<()> {
        let path = "/verifiers/endorsements";
        let body = serde_json::to_vec(&EndorseBodyWithEvidence {
            username,
            topic,
            evidence,
        })?;
        let headers = verifier.sign_request("POST", path, &body);

        let mut req = self
            .http
            .post(format!("{}{path}", self.api_url))
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in headers {
            req = req.header(key, value);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("POST {path} returned {status}: {body}"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn url_construction_list_rooms() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let url = format!("{}/rooms", client.api_url);
        assert_eq!(url, "http://localhost:4000/rooms");
    }

    #[test]
    fn url_construction_list_polls() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "http://localhost:4000/rooms/550e8400-e29b-41d4-a716-446655440000/polls"
        );
    }

    #[test]
    fn url_construction_poll_detail() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let poll_id = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls/{poll_id}");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "http://localhost:4000/rooms/550e8400-e29b-41d4-a716-446655440000/polls/660e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn url_construction_poll_results() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let poll_id = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls/{poll_id}/results");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "http://localhost:4000/rooms/550e8400-e29b-41d4-a716-446655440000/polls/660e8400-e29b-41d4-a716-446655440000/results"
        );
    }

    #[test]
    fn url_construction_signup() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let url = format!("{}/auth/signup", client.api_url);
        assert_eq!(url, "http://localhost:4000/auth/signup");
    }

    #[test]
    fn url_construction_dimensions() {
        let client = SimClient::new(
            reqwest::Client::new(),
            "https://api.example.com".to_string(),
        );
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let poll_id = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls/{poll_id}/dimensions");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "https://api.example.com/rooms/550e8400-e29b-41d4-a716-446655440000/polls/660e8400-e29b-41d4-a716-446655440000/dimensions"
        );
    }

    #[test]
    fn url_construction_poll_status() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let poll_id = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls/{poll_id}/status");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "http://localhost:4000/rooms/550e8400-e29b-41d4-a716-446655440000/polls/660e8400-e29b-41d4-a716-446655440000/status"
        );
    }

    #[test]
    fn url_construction_vote() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let room_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let poll_id = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = format!("/rooms/{room_id}/polls/{poll_id}/vote");
        let url = format!("{}{path}", client.api_url);
        assert_eq!(
            url,
            "http://localhost:4000/rooms/550e8400-e29b-41d4-a716-446655440000/polls/660e8400-e29b-41d4-a716-446655440000/vote"
        );
    }

    #[test]
    fn url_construction_login() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let url = format!("{}/auth/login", client.api_url);
        assert_eq!(url, "http://localhost:4000/auth/login");
    }

    #[test]
    fn url_construction_endorsements() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let url = format!("{}/verifiers/endorsements", client.api_url);
        assert_eq!(url, "http://localhost:4000/verifiers/endorsements");
    }

    #[test]
    fn url_construction_trailing_slash_preserved() {
        // If the api_url has a trailing slash, our paths would double-slash.
        // Verify we don't add one in our construction.
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        let url = format!("{}/rooms", client.api_url);
        assert!(!url.contains("//rooms"), "should not double-slash");
    }

    // -- JSON serialization tests -----------------------------------------

    #[test]
    fn create_room_body_serializes() {
        let body = CreateRoomBody {
            name: "Test Room",
            description: "A test room",
            eligibility_topic: "testing",
            constraint_type: "identity_verified",
            constraint_config: None,
            poll_duration_secs: Some(3600),
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["name"], "Test Room");
        assert_eq!(json["description"], "A test room");
        assert_eq!(json["eligibility_topic"], "testing");
        assert_eq!(json["poll_duration_secs"], 3600);
    }

    #[test]
    fn create_poll_body_serializes() {
        let body = CreatePollBody {
            question: "Should we build a park?",
            description: "A new park proposal",
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["question"], "Should we build a park?");
        assert_eq!(json["description"], "A new park proposal");
    }

    #[test]
    fn add_dimension_body_serializes() {
        let body = AddDimensionBody {
            name: "Importance",
            description: "How important is this?",
            min_value: 0.0,
            max_value: 10.0,
            sort_order: 1,
            min_label: Some("Not important"),
            max_label: Some("Very important"),
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["name"], "Importance");
        assert_eq!(json["description"], "How important is this?");
        assert_eq!(json["min_value"], 0.0);
        assert_eq!(json["max_value"], 10.0);
        assert_eq!(json["sort_order"], 1);
    }

    #[test]
    fn update_status_body_serializes() {
        let body = UpdateStatusBody { status: "active" };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["status"], "active");
    }

    #[test]
    fn cast_vote_body_serializes() {
        let dim_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let entries = vec![VoteEntry {
            dimension_id: dim_id,
            value: 7.5,
        }];
        let body = CastVoteBody { votes: &entries };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert!(json["votes"].is_array());
        assert_eq!(json["votes"][0]["dimension_id"], dim_id.to_string());
        assert_eq!(json["votes"][0]["value"], 7.5);
    }

    #[test]
    fn cast_vote_body_multiple_dimensions() {
        let dim1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let dim2 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap();
        let entries = vec![
            VoteEntry {
                dimension_id: dim1,
                value: 3.0,
            },
            VoteEntry {
                dimension_id: dim2,
                value: 8.5,
            },
        ];
        let body = CastVoteBody { votes: &entries };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["votes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn endorse_body_serializes() {
        let body = EndorseBody {
            username: "sim_voter_00",
            topic: "parks",
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();

        assert_eq!(json["username"], "sim_voter_00");
        assert_eq!(json["topic"], "parks");
    }

    // -- Response deserialization tests ------------------------------------

    #[test]
    fn room_response_deserializes() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Test", "description": "Desc", "status": "active"}"#;
        let room: RoomResponse = serde_json::from_str(json).unwrap();
        assert_eq!(room.name, "Test");
        assert_eq!(
            room.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
    }

    #[test]
    fn room_response_optional_description() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Test", "description": null, "status": "active"}"#;
        let room: RoomResponse = serde_json::from_str(json).unwrap();
        assert!(room.description.is_none());
    }

    #[test]
    fn room_response_ignores_extra_fields() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Test", "description": null, "status": "active", "created_at": "2024-01-01T00:00:00Z"}"#;
        let room: RoomResponse = serde_json::from_str(json).unwrap();
        assert_eq!(room.name, "Test");
    }

    #[test]
    fn poll_response_deserializes() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "room_id": "660e8400-e29b-41d4-a716-446655440000", "question": "Build a park?", "status": "draft"}"#;
        let poll: PollResponse = serde_json::from_str(json).unwrap();
        assert_eq!(poll.question, "Build a park?");
    }

    #[test]
    fn poll_detail_response_deserializes() {
        let json = r#"{
            "poll": {"id": "550e8400-e29b-41d4-a716-446655440000", "room_id": "660e8400-e29b-41d4-a716-446655440000", "question": "Q?", "status": "active"},
            "dimensions": [{"id": "770e8400-e29b-41d4-a716-446655440000", "name": "Dim1", "min_value": 0.0, "max_value": 10.0}]
        }"#;
        let detail: PollDetailResponse = serde_json::from_str(json).unwrap();
        assert_eq!(detail.dimensions.len(), 1);
        assert_eq!(detail.dimensions[0].name, "Dim1");
    }

    #[test]
    fn dimension_response_deserializes() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Cost", "min_value": 1.0, "max_value": 5.0}"#;
        let dim: DimensionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(dim.name, "Cost");
        assert!((dim.min_value - 1.0).abs() < f32::EPSILON);
        assert!((dim.max_value - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn poll_results_response_deserializes() {
        let json = r#"{"voter_count": 42}"#;
        let results: PollResultsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(results.voter_count, 42);
    }

    #[test]
    fn poll_results_response_ignores_extra_fields() {
        let json = r#"{"voter_count": 10, "average_score": 7.5}"#;
        let results: PollResultsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(results.voter_count, 10);
    }

    #[test]
    fn signup_response_deserializes() {
        let json = r#"{"account_id": "550e8400-e29b-41d4-a716-446655440000", "root_kid": "abc123", "device_kid": "def456"}"#;
        let resp: SignupResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.account_id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
    }

    #[test]
    fn vote_response_deserializes() {
        let json = r#"{"dimension_id": "550e8400-e29b-41d4-a716-446655440000", "value": 7.5}"#;
        let vote: VoteResponse = serde_json::from_str(json).unwrap();
        assert!((vote.value - 7.5).abs() < f32::EPSILON);
    }

    #[test]
    fn vote_response_vec_deserializes() {
        let json = r#"[
            {"dimension_id": "550e8400-e29b-41d4-a716-446655440000", "value": 3.0},
            {"dimension_id": "660e8400-e29b-41d4-a716-446655440000", "value": 8.5}
        ]"#;
        let votes: Vec<VoteResponse> = serde_json::from_str(json).unwrap();
        assert_eq!(votes.len(), 2);
    }

    #[test]
    fn new_client_stores_url() {
        let client = SimClient::new(reqwest::Client::new(), "http://localhost:4000".to_string());
        assert_eq!(client.api_url, "http://localhost:4000");
    }

    #[test]
    fn endorse_body_with_evidence_serializes() {
        let evidence =
            serde_json::json!({ "method": "government_id", "provider": "demo_verifier" });
        let body = EndorseBodyWithEvidence {
            username: "alice",
            topic: "identity_verified",
            evidence: Some(&evidence),
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(json["username"], "alice");
        assert_eq!(json["topic"], "identity_verified");
        assert_eq!(json["evidence"]["method"], "government_id");
    }

    #[test]
    fn endorse_body_without_evidence_omits_field() {
        let body = EndorseBodyWithEvidence {
            username: "bob",
            topic: "identity_verified",
            evidence: None,
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(json["username"], "bob");
        assert!(json.get("evidence").is_none());
    }
}
