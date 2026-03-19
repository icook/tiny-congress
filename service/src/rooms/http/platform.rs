//! HTTP handlers for room CRUD (platform-level, not engine-specific)

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tc_engine_api::engine::{EngineContext, EngineRegistry};
use uuid::Uuid;

use super::{CreateRoomRequest, RoomResponse};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::http::ErrorResponse;
use crate::rooms::repo::RoomRecord;
use crate::rooms::service::{RoomError, RoomsService};

// ─── Room handlers ─────────────────────────────────────────────────────────

pub async fn list_rooms(Extension(service): Extension<Arc<dyn RoomsService>>) -> impl IntoResponse {
    match service.list_rooms(Some("open")).await {
        Ok(rooms) => {
            let rooms: Vec<_> = rooms.into_iter().map(room_to_response).collect();
            (StatusCode::OK, Json(rooms)).into_response()
        }
        Err(e) => room_error_response(e),
    }
}

pub async fn get_room(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match service.get_room(room_id).await {
        Ok(room) => (StatusCode::OK, Json(room_to_response(room))).into_response(),
        Err(e) => room_error_response(e),
    }
}

pub async fn create_room(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Extension(engine_registry): Extension<Arc<EngineRegistry>>,
    Extension(engine_ctx): Extension<EngineContext>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreateRoomRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    // Validate engine type and configuration before persisting the room.
    let Some(engine) = engine_registry.get(&req.engine_type) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Unknown engine type: {}", req.engine_type),
            }),
        )
            .into_response();
    };
    if let Err(e) = engine.validate_config(&req.engine_config) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response();
    }

    let room = match service
        .create_room(
            &req.name,
            req.description.as_deref(),
            &req.eligibility_topic,
            req.poll_duration_secs,
            &req.constraint_type,
            &req.constraint_config,
        )
        .await
    {
        Ok(room) => room,
        Err(e) => return room_error_response(e),
    };

    // Notify the engine that a room was created so it can set up engine-specific state.
    if let Err(e) = engine
        .on_room_created(room.id, &req.engine_config, &engine_ctx)
        .await
    {
        tracing::error!(
            room_id = %room.id,
            engine_type = %req.engine_type,
            error = %e,
            "on_room_created hook failed"
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Room created but engine initialisation failed".to_string(),
            }),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(room_to_response(room))).into_response()
}

pub async fn get_capacity(
    Extension(service): Extension<Arc<dyn RoomsService>>,
) -> impl IntoResponse {
    match service.rooms_needing_content().await {
        Ok(rooms) => {
            let rooms: Vec<_> = rooms.into_iter().map(room_to_response).collect();
            (StatusCode::OK, Json(rooms)).into_response()
        }
        Err(e) => room_error_response(e),
    }
}

// ─── Response converters ──────────────────────────────────────────────────

fn room_to_response(r: RoomRecord) -> RoomResponse {
    RoomResponse {
        id: r.id,
        name: r.name,
        description: r.description,
        eligibility_topic: r.eligibility_topic,
        status: r.status,
        poll_duration_secs: r.poll_duration_secs,
        created_at: r.created_at.to_rfc3339(),
        engine_type: r.engine_type,
        engine_config: r.engine_config,
    }
}

// ─── Error mappers ────────────────────────────────────────────────────────

fn room_error_response(e: RoomError) -> axum::response::Response {
    match e {
        RoomError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        RoomError::RoomNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Room not found".to_string(),
            }),
        )
            .into_response(),
        RoomError::DuplicateRoomName => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Room name already exists".to_string(),
            }),
        )
            .into_response(),
        RoomError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
            }),
        )
            .into_response(),
    }
}
