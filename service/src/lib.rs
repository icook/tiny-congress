#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

pub mod build_info;
pub mod config;
pub mod db;
pub mod graphql;
pub mod http;
pub mod identity;
pub mod rest;
