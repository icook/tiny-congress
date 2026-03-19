//! Background consumer for the lifecycle message queue.
//!
//! The implementation now lives in [`tc_engine_polling::lifecycle`].
//! This module re-exports for backward compatibility.

pub use tc_engine_polling::lifecycle::spawn_lifecycle_consumer;
