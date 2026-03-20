// lint-patterns:allow-no-utoipa — tracked by #907
//! HTTP handlers for room CRUD (platform-level, not engine-specific)

use std::sync::Arc;

use crate::http::{bad_request, internal_error, not_found};
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sqlx::PgPool;
use tc_engine_api::engine::{EngineContext, EngineRegistry};
use uuid::Uuid;

use tc_engine_api::constraints::build_constraint;

use super::{
    AssignRoleRequest, AssignRoleResponse, CreateRoomRequest, MyCapabilitiesResponse, RoomResponse,
};
use crate::http::ErrorResponse;
use crate::identity::http::auth::AuthenticatedDevice;
use crate::rooms::content_filter::{ContentFilter, FilterResult};
use crate::rooms::repo::suggestions;
use crate::rooms::repo::RoomRecord;
use crate::rooms::service::{RoomError, RoomsService};
use crate::trust::graph_reader::TrustRepoGraphReader;
use crate::trust::repo::TrustRepo;

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

    // Auto-configure endorsed_by_user constraint with creator as endorser
    // when no explicit endorser_id is provided.
    let constraint_config = if req.constraint_type == "endorsed_by_user"
        && req.constraint_config.get("endorser_id").is_none()
    {
        serde_json::json!({ "endorser_id": auth.account_id.to_string() })
    } else {
        req.constraint_config.clone()
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
            &constraint_config,
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

// ─── Capabilities endpoint ────────────────────────────────────────────────

pub async fn my_capabilities(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    Extension(pool): Extension<PgPool>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let room = match service.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => return room_error_response(e),
    };

    // Owner check
    if room.owner_id == Some(auth.account_id) {
        return (
            StatusCode::OK,
            Json(MyCapabilitiesResponse {
                role: "owner".to_string(),
                can_vote: true,
                can_configure: true,
                reason: None,
                next_step: None,
            }),
        )
            .into_response();
    }

    // Check for explicit role assignment (layer 2: per-room elevation)
    let assigned_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM rooms__role_assignments WHERE room_id = $1 AND account_id = $2",
    )
    .bind(room_id)
    .bind(auth.account_id)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    if let Some(role) = assigned_role {
        return (
            StatusCode::OK,
            Json(MyCapabilitiesResponse {
                role,
                can_vote: true,
                can_configure: false,
                reason: None,
                next_step: None,
            }),
        )
            .into_response();
    }

    // Participant check: evaluate room constraint (layer 1: platform endorsement)
    let trust_reader = TrustRepoGraphReader::new(trust_repo);
    let constraint = match build_constraint(&room.constraint_type, &room.constraint_config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(room_id = %room_id, "failed to build constraint: {e}");
            return internal_error();
        }
    };

    match constraint.check(auth.account_id, &trust_reader).await {
        Ok(eligibility) if eligibility.is_eligible => (
            StatusCode::OK,
            Json(MyCapabilitiesResponse {
                role: "participant".to_string(),
                can_vote: true,
                can_configure: false,
                reason: None,
                next_step: None,
            }),
        )
            .into_response(),
        Ok(eligibility) => {
            let next_step = if room.constraint_type == "endorsed_by_user" {
                Some("Ask the room owner to endorse you".to_string())
            } else {
                Some("Complete identity verification or get endorsed".to_string())
            };
            (
                StatusCode::OK,
                Json(MyCapabilitiesResponse {
                    role: "none".to_string(),
                    can_vote: false,
                    can_configure: false,
                    reason: eligibility.reason,
                    next_step,
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(room_id = %room_id, "constraint check error: {e}");
            internal_error()
        }
    }
}

// ─── Role assignment endpoint ─────────────────────────────────────────────

pub async fn assign_role(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Extension(pool): Extension<PgPool>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: AssignRoleRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    // Only the room owner can assign roles
    let room = match service.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => return room_error_response(e),
    };

    if room.owner_id != Some(auth.account_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Only the room owner can assign roles".to_string(),
            }),
        )
            .into_response();
    }

    // Upsert role assignment
    match sqlx::query(
        "INSERT INTO rooms__role_assignments (room_id, account_id, role, assigned_by) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (room_id, account_id) \
         DO UPDATE SET role = EXCLUDED.role, assigned_by = EXCLUDED.assigned_by, assigned_at = now()",
    )
    .bind(room_id)
    .bind(req.account_id)
    .bind(&req.role)
    .bind(auth.account_id)
    .execute(&pool)
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(AssignRoleResponse {
                room_id,
                account_id: req.account_id,
                role: req.role,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(room_id = %room_id, "role assignment failed: {e}");
            internal_error()
        }
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

// ─── Suggestion handlers ─────────────────────────────────────────────────

const DAILY_SUGGESTION_LIMIT: i64 = 3;

pub async fn create_suggestion(
    Extension(pool): Extension<PgPool>,
    Extension(content_filter): Extension<Arc<dyn ContentFilter>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: super::CreateSuggestionRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let text = req.suggestion_text.trim().to_string();
    if text.is_empty() || text.len() > 500 {
        return bad_request("Suggestion must be 1-500 characters");
    }

    // Rate limit check
    let daily_count =
        match suggestions::count_user_suggestions_today(&pool, room_id, auth.account_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(room_id = %room_id, "suggestion count failed: {e}");
                return internal_error();
            }
        };
    if daily_count >= DAILY_SUGGESTION_LIMIT {
        return bad_request("Daily suggestion limit reached (3)");
    }

    // Content filter
    let filter_result = content_filter.check(&text).await;
    let (status, filter_reason) = match filter_result {
        FilterResult::Accept => ("queued", None),
        FilterResult::Reject { reason } => ("rejected", Some(reason)),
    };

    match suggestions::create_suggestion(
        &pool,
        room_id,
        auth.account_id,
        &text,
        status,
        filter_reason.as_deref(),
    )
    .await
    {
        Ok(s) => (StatusCode::CREATED, Json(suggestion_to_response(s))).into_response(),
        Err(e) => {
            tracing::error!(room_id = %room_id, "suggestion creation failed: {e}");
            internal_error()
        }
    }
}

pub async fn list_suggestions(
    Extension(pool): Extension<PgPool>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match suggestions::list_suggestions(&pool, room_id).await {
        Ok(suggestion_list) => {
            let resp: Vec<_> = suggestion_list
                .into_iter()
                .map(suggestion_to_response)
                .collect();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => {
            tracing::error!(room_id = %room_id, "list suggestions failed: {e}");
            internal_error()
        }
    }
}

fn suggestion_to_response(s: suggestions::SuggestionRecord) -> super::SuggestionResponse {
    super::SuggestionResponse {
        id: s.id,
        room_id: s.room_id,
        account_id: s.account_id,
        suggestion_text: s.suggestion_text,
        status: s.status,
        filter_reason: s.filter_reason,
        evidence_ids: s.evidence_ids,
        created_at: s.created_at.to_rfc3339(),
        processed_at: s.processed_at.map(|t| t.to_rfc3339()),
    }
}

// ─── Error mappers ────────────────────────────────────────────────────────

fn room_error_response(e: RoomError) -> axum::response::Response {
    match e {
        RoomError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        RoomError::RoomNotFound => not_found("Room not found"),
        RoomError::DuplicateRoomName => crate::http::conflict("Room name already exists"),
        RoomError::Internal(_) => internal_error(),
    }
}
