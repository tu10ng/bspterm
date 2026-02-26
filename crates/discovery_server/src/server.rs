use std::net::SocketAddr;
use std::time::Duration;

use axum::{Router, routing::{delete, get, post}};
use tokio::net::TcpListener;

use crate::routes::{get_users, health_check, register_user, unregister_user};
use crate::state::AppState;

pub const DEFAULT_PORT: u16 = 53720;
pub const CLEANUP_INTERVAL_SECS: u64 = 15;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/users/register", post(register_user))
        .route("/api/v1/users", get(get_users))
        .route("/api/v1/users/{instance_id}", delete(unregister_user))
        .with_state(state)
}

pub async fn run_server(bind: &str, port: u16) -> anyhow::Result<()> {
    let state = AppState::new();

    let cleanup_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECS)).await;
            let removed = cleanup_state.cleanup_expired().await;
            if removed > 0 {
                log::info!("Cleaned up {} expired users", removed);
            }
        }
    });

    let app = create_router(state);

    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    log::info!("Discovery server listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_check() {
        let state = AppState::new();
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_register_and_get_users() {
        let state = AppState::new();
        let app = create_router(state);

        let register_body = serde_json::json!({
            "employee_id": "12345",
            "name": "Test User",
            "instance_id": "550e8400-e29b-41d4-a716-446655440000",
            "ip_addresses": ["192.168.1.100"],
            "active_sessions": []
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&register_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let register_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(register_response["success"].as_bool().unwrap());
        assert!(register_response["is_new"].as_bool().unwrap());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let users_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let users = users_response["users"].as_array().unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["name"], "Test User");
    }
}
