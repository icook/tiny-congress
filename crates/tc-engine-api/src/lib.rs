//! Plugin contract for `TinyCongress` room engines.
//!
//! This crate defines the traits and types that room engine plugins must
//! implement. It is intentionally kept dependency-light so that engine
//! implementations can depend on it without pulling in the full service.

pub mod constraints;
pub mod engine;
pub mod error;
pub mod trust;
