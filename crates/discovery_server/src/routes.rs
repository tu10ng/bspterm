use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::{AppState, UserInfo, UserRegistration};

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub is_new: bool,
}

#[derive(Debug, Deserialize)]
pub struct GetUsersQuery {
    pub exclude: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct GetUsersResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Serialize)]
pub struct UnregisterResponse {
    pub success: bool,
}

pub async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<UserRegistration>,
) -> impl IntoResponse {
    log::debug!(
        "Register user: {} ({})",
        payload.name,
        payload.instance_id
    );

    let is_new = state.register_user(payload).await;

    Json(RegisterResponse {
        success: true,
        is_new,
    })
}

pub async fn get_users(
    State(state): State<AppState>,
    Query(query): Query<GetUsersQuery>,
) -> impl IntoResponse {
    let users = state.get_users(query.exclude).await;

    log::debug!("Get users: {} online (exclude: {:?})", users.len(), query.exclude);

    Json(GetUsersResponse { users })
}

pub async fn unregister_user(
    State(state): State<AppState>,
    Path(instance_id): Path<Uuid>,
) -> impl IntoResponse {
    log::debug!("Unregister user: {}", instance_id);

    let removed = state.unregister_user(instance_id).await;

    if removed {
        (StatusCode::OK, Json(UnregisterResponse { success: true }))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(UnregisterResponse { success: false }),
        )
    }
}

pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
