//! `TinyCongress` API library.
//!
//! This crate provides the core functionality for the `TinyCongress` GraphQL API,
//! including database access, configuration, and HTTP handlers.

#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

/// Build metadata and version information.
pub mod build_info;
/// Application configuration loading and validation.
pub mod config;
/// Database connection pool and migrations.
pub mod db;
/// GraphQL schema and resolvers.
pub mod graphql;
/// HTTP server setup and middleware.
pub mod http;
/// Identity and authentication domain.
pub mod identity;
/// REST API endpoints.
pub mod rest;
