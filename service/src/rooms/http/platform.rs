//! HTTP handlers for room CRUD (platform-level, not engine-specific)

use std::sync::Arc;

use crate::http::{bad_request, internal_error, not_found};
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tc_engine_api::engine::{EngineContext, EngineRegistry};
use uuid::Uuid;

use super::{CreateRoomRequest, RoomResponse};
use crate::http::ErrorResponse;
use crate::identity::http::auth::AuthenticatedDevice;
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
        return bad_request(&format!("Unknown engine type: {}", req.engine_type));
    };
    if let Err(e) = engine.validate_config(&req.engine_config) {
        return bad_request(&e.to_string());
    }

    let room = match service
        .create_room(
            &req.name,
            req.description.as_deref(),
            &req.eligibility_topic,
            req.poll_duration_secs,
            &req.constraint_type,
            &req.constraint_config,
            Some(auth.account_id),
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
        return internal_error();
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
        owner_id: r.owner_id,
        constraint_type: r.constraint_type,
    }
}

// ─── Error mappers ────────────────────────────────────────────────────────

fn room_error_response(e: RoomError) -> axum::response::Response {
    match e {
        RoomError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        RoomError::RoomNotFound => not_found("Room not found"),
        RoomError::DuplicateRoomName => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Room name already exists".to_string(),
            }),
        )
            .into_response(),
        RoomError::Internal(_) => internal_error(),
    }
}
