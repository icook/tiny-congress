//! Core room engine plugin trait and supporting types.
//!
//! This module defines the [`RoomEngine`] trait that all room engine plugins
//! implement, plus the context types ([`EngineContext`], [`PlatformState`])
//! they receive and the [`EngineRegistry`] that collects them.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::constraints::ConstraintRegistry;
use crate::error::EngineError;
use crate::trust::TrustGraphReader;

// ---------------------------------------------------------------------------
// EngineMetadata
// ---------------------------------------------------------------------------

/// Human-readable metadata for a room engine plugin.
#[derive(Debug, Clone, Serialize)]
pub struct EngineMetadata {
    /// User-facing name of the engine (e.g. "Ranked-Choice Poll").
    pub display_name: String,
    /// Short description of what this engine does.
    pub description: String,
}

// ---------------------------------------------------------------------------
// RoomRecord
// ---------------------------------------------------------------------------

/// Minimal room record exposed to engines via [`RoomLifecycle`].
///
/// This is a read-only snapshot of the room's current state — engines receive
/// it when they need to inspect room configuration without coupling to the
/// service layer's full `Room` type.
#[derive(Debug, Clone, Serialize)]
pub struct RoomRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub engine_type: String,
    pub engine_config: serde_json::Value,
    pub constraint_type: String,
    pub constraint_config: serde_json::Value,
    pub poll_duration_secs: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// RoomLifecycle
// ---------------------------------------------------------------------------

/// Service-layer operations that engines may invoke on rooms.
///
/// The concrete implementation lives in the service layer; engines receive
/// this as a trait object so they stay decoupled from the full service.
#[async_trait::async_trait]
pub trait RoomLifecycle: Send + Sync {
    /// Fetch a room by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the room does not exist or on infrastructure failure.
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, anyhow::Error>;

    /// Close a room, preventing further participation.
    ///
    /// # Errors
    ///
    /// Returns an error if the room does not exist, is already closed, or on
    /// infrastructure failure.
    async fn close_room(&self, room_id: Uuid) -> Result<(), anyhow::Error>;
}

// ---------------------------------------------------------------------------
// EngineContext
// ---------------------------------------------------------------------------

/// Shared resources passed to engine methods.
///
/// Every engine receives a clone of this context at startup and on each
/// room-creation callback. It provides access to the database, trust graph,
/// constraint registry, and room lifecycle operations.
#[derive(Clone)]
pub struct EngineContext {
    pub pool: PgPool,
    pub trust_reader: Arc<dyn TrustGraphReader>,
    pub constraints: Arc<ConstraintRegistry>,
    pub room_lifecycle: Arc<dyn RoomLifecycle>,
}

// ---------------------------------------------------------------------------
// PlatformState
// ---------------------------------------------------------------------------

/// Top-level application state threaded through Axum routers.
///
/// Engine routers receive this as their state type, giving them access to
/// both the engine registry (for cross-engine lookups) and the shared context.
#[derive(Clone)]
pub struct PlatformState {
    pub engine_registry: Arc<EngineRegistry>,
    pub engine_ctx: EngineContext,
}

// ---------------------------------------------------------------------------
// RoomEngine trait
// ---------------------------------------------------------------------------

/// Plugin trait that all room engines implement.
///
/// A `RoomEngine` is a self-contained module that owns the behaviour for one
/// type of room (e.g. ranked-choice polling, deliberation). The platform
/// discovers engines via [`EngineRegistry`] and dispatches to them based on
/// the room's `engine_type` field.
///
/// # Object safety
///
/// `on_room_created` is async, which requires `#[async_trait]` for object
/// safety (`dyn RoomEngine`). The synchronous methods are plain `fn`.
#[async_trait::async_trait]
pub trait RoomEngine: Send + Sync {
    /// Unique identifier for this engine type (e.g. `"poll"`).
    fn engine_type(&self) -> &'static str;

    /// Human-readable metadata for API discovery endpoints.
    fn metadata(&self) -> EngineMetadata;

    /// Axum router fragment for this engine's HTTP endpoints.
    ///
    /// The returned router is nested under `/engines/{engine_type}` by the
    /// platform. It receives [`PlatformState`] as its state.
    fn routes(&self) -> axum::Router<PlatformState>;

    /// JSON Schema describing the engine-specific configuration blob.
    fn config_schema(&self) -> serde_json::Value;

    /// Validate an engine configuration blob before room creation.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidInput`] if the config is malformed.
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), EngineError>;

    /// Called after a room is created with this engine type.
    ///
    /// Engines use this hook to set up engine-specific state (e.g. create
    /// a poll record, initialise round tracking, etc.).
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] if setup fails.
    async fn on_room_created(
        &self,
        room_id: Uuid,
        config: &serde_json::Value,
        ctx: &EngineContext,
    ) -> Result<(), EngineError>;

    /// Start long-running background tasks for this engine.
    ///
    /// Called once at application startup. The returned join handles are
    /// held by the platform and aborted on shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if task spawning fails.
    fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error>;
}

// ---------------------------------------------------------------------------
// EngineRegistry
// ---------------------------------------------------------------------------

/// Registry of available room engine plugins.
///
/// Engines are registered at application startup. The platform uses the
/// registry to look up the correct engine when creating or interacting
/// with rooms.
pub struct EngineRegistry {
    engines: HashMap<String, Arc<dyn RoomEngine>>,
}

impl EngineRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }

    /// Register an engine plugin.
    ///
    /// If an engine with the same `engine_type` is already registered, the
    /// new one replaces it.
    pub fn register(&mut self, engine: impl RoomEngine + 'static) {
        let key = engine.engine_type().to_string();
        self.engines.insert(key, Arc::new(engine));
    }

    /// Look up an engine by type string.
    #[must_use]
    pub fn get(&self, engine_type: &str) -> Option<Arc<dyn RoomEngine>> {
        self.engines.get(engine_type).cloned()
    }

    /// Return all registered engines.
    #[must_use]
    pub fn all(&self) -> Vec<Arc<dyn RoomEngine>> {
        self.engines.values().cloned().collect()
    }

    /// Return the type strings of all registered engines.
    #[must_use]
    pub fn engine_types(&self) -> Vec<&str> {
        self.engines.keys().map(String::as_str).collect()
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEngine {
        engine_type: &'static str,
        display_name: &'static str,
    }

    impl MockEngine {
        fn new(engine_type: &'static str, display_name: &'static str) -> Self {
            Self {
                engine_type,
                display_name,
            }
        }
    }

    #[async_trait::async_trait]
    impl RoomEngine for MockEngine {
        fn engine_type(&self) -> &'static str {
            self.engine_type
        }

        fn metadata(&self) -> EngineMetadata {
            EngineMetadata {
                display_name: self.display_name.to_string(),
                description: String::new(),
            }
        }

        fn routes(&self) -> axum::Router<PlatformState> {
            axum::Router::new()
        }

        fn config_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn validate_config(&self, _config: &serde_json::Value) -> Result<(), EngineError> {
            Ok(())
        }

        async fn on_room_created(
            &self,
            _room_id: Uuid,
            _config: &serde_json::Value,
            _ctx: &EngineContext,
        ) -> Result<(), EngineError> {
            Ok(())
        }

        fn start(
            &self,
            _ctx: EngineContext,
        ) -> Result<Vec<tokio::task::JoinHandle<()>>, anyhow::Error> {
            Ok(vec![])
        }
    }

    #[test]
    fn get_returns_none_for_unregistered_type() {
        let registry = EngineRegistry::new();
        assert!(registry.get("poll").is_none());
    }

    #[test]
    fn get_returns_registered_engine() {
        let mut registry = EngineRegistry::new();
        registry.register(MockEngine::new("poll", "Poll"));
        let engine = registry.get("poll").unwrap();
        assert_eq!(engine.engine_type(), "poll");
    }

    #[test]
    fn register_replaces_engine_with_same_type() {
        let mut registry = EngineRegistry::new();
        registry.register(MockEngine::new("poll", "Old Poll"));
        registry.register(MockEngine::new("poll", "New Poll"));
        let engine = registry.get("poll").unwrap();
        assert_eq!(engine.metadata().display_name, "New Poll");
    }

    #[test]
    fn all_returns_all_registered_engines() {
        let mut registry = EngineRegistry::new();
        registry.register(MockEngine::new("poll", "Poll"));
        registry.register(MockEngine::new("deliberation", "Deliberation"));
        assert_eq!(registry.all().len(), 2);
    }

    #[test]
    fn engine_types_returns_type_strings_for_all_engines() {
        let mut registry = EngineRegistry::new();
        registry.register(MockEngine::new("poll", "Poll"));
        registry.register(MockEngine::new("deliberation", "Deliberation"));
        let mut types = registry.engine_types();
        types.sort();
        assert_eq!(types, vec!["deliberation", "poll"]);
    }

    #[test]
    fn default_creates_empty_registry() {
        let registry = EngineRegistry::default();
        assert!(registry.all().is_empty());
        assert!(registry.engine_types().is_empty());
    }
}
