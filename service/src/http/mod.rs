//! HTTP utilities and middleware.
//!
//! This module provides shared HTTP functionality used by the application server.

pub mod security;

pub use security::{build_security_headers, security_headers_middleware};
