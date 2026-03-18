//! Axum routing layer for the room engine plugin system.
//!
//! This module provides:
//! - An `engines_router()` function that builds the HTTP routes for engine discovery
//! - A `GET /engines` endpoint listing all registered engines and their metadata
//!
//! The actual per-room delegation (looking up a room's `engine_type` from the DB
//! and routing to the matching engine) will be added when the polling engine is
//! extracted.

use std::sync::Arc;

use axum::{
    extract::Extension, http::StatusCode, response::IntoResponse, routing::get, Json, Router,
};
use serde::Serialize;
use tc_engine_api::engine::EngineRegistry;

/// Response type for the `GET /engines` endpoint.
#[derive(Debug, Serialize)]
pub struct EngineListResponse {
    pub engines: Vec<EngineInfo>,
}

/// Summary info for a single registered engine.
#[derive(Debug, Serialize)]
pub struct EngineInfo {
    pub engine_type: String,
    pub display_name: String,
    pub description: String,
}

/// Build the Axum router for engine discovery endpoints.
///
/// Currently provides:
/// - `GET /engines` — list all registered engines
pub fn engines_router() -> Router {
    Router::new().route("/engines", get(list_engines))
}

/// Handler: list all registered engines with their metadata.
async fn list_engines(Extension(registry): Extension<Arc<EngineRegistry>>) -> impl IntoResponse {
    let engines: Vec<EngineInfo> = registry
        .all()
        .into_iter()
        .map(|e| {
            let meta = e.metadata();
            EngineInfo {
                engine_type: e.engine_type().to_string(),
                display_name: meta.display_name,
                description: meta.description,
            }
        })
        .collect();

    (StatusCode::OK, Json(EngineListResponse { engines }))
}
