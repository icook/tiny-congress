//! Bot subsystem for polling rooms.
//!
//! A bot automates poll creation and research for a room based on configuration
//! stored in `rooms__rooms.engine_config.bot`.

pub mod config;
pub mod worker;

pub use config::{BotConfig, Quality, RunMode};
