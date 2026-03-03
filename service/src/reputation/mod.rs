//! Reputation module for `TinyCongress`
//!
//! Provides the endorsement system that gates room participation.
//! Verifier service accounts issue endorsements to users for specific topics
//! (e.g., `identity_verified`).

pub mod http;
pub mod repo;
pub mod service;
