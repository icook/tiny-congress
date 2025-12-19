//! Data types for Congress API responses.

use serde::{Deserialize, Serialize};

/// A member of Congress (Senator or Representative).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Member {
    /// Bioguide ID (e.g., "A000360")
    pub id: String,
    /// Full name
    pub name: String,
    /// State abbreviation (e.g., "TN")
    pub state: String,
    /// Party affiliation (e.g., "R", "D", "I")
    pub party: String,
    /// Chamber ("Senate" or "House")
    pub chamber: String,
}

/// Response from the members endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembersResponse {
    pub members: Vec<Member>,
}

/// Response from the member detail endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberResponse {
    pub member: Member,
}
