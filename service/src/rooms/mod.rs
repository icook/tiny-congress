//! Rooms module for `TinyCongress`
//!
//! Provides the room and polling engine. Rooms contain polls with
//! multi-dimensional voting. Eligibility is checked via the endorsement system.

pub mod content_filter;
pub mod http;
pub mod lifecycle;
pub mod repo;
pub mod service;
