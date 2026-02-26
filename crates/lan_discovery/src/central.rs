use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;
use futures::AsyncReadExt;
use http_client::{AsyncBody, HttpClient, Method, Request};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::broadcast::{ActiveSessionInfo, SessionProtocol};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserRegistration {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    pub success: bool,
    #[allow(dead_code)]
    pub is_new: bool,
}

#[derive(Debug, Deserialize)]
struct GetUsersResponse {
    pub users: Vec<UserInfo>,
}

pub struct CentralDiscoveryClient {
    server_url: String,
    http_client: Arc<dyn HttpClient>,
}

impl CentralDiscoveryClient {
    pub fn new(server_url: String, http_client: Arc<dyn HttpClient>) -> Self {
        Self {
            server_url,
            http_client,
        }
    }

    pub async fn register(&self, registration: &UserRegistration) -> Result<()> {
        let url = format!("{}/api/v1/users/register", self.server_url);
        let body = serde_json::to_vec(registration)?;

        let request = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/json")
            .body(AsyncBody::from(body))?;

        let response = self.http_client.send(request).await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to register: HTTP {}",
                response.status()
            ));
        }

        let mut body_bytes = Vec::new();
        response.into_body().read_to_end(&mut body_bytes).await?;
        let response: RegisterResponse = serde_json::from_slice(&body_bytes)?;

        if !response.success {
            return Err(anyhow::anyhow!("Registration failed"));
        }

        Ok(())
    }

    pub async fn get_users(&self, exclude_instance_id: Uuid) -> Result<Vec<UserInfo>> {
        let url = format!(
            "{}/api/v1/users?exclude={}",
            self.server_url, exclude_instance_id
        );

        let request = Request::builder()
            .method(Method::GET)
            .uri(&url)
            .body(AsyncBody::empty())?;

        let response = self.http_client.send(request).await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to get users: HTTP {}",
                response.status()
            ));
        }

        let mut body_bytes = Vec::new();
        response.into_body().read_to_end(&mut body_bytes).await?;
        let response: GetUsersResponse = serde_json::from_slice(&body_bytes)?;

        Ok(response.users)
    }

    #[allow(dead_code)]
    pub async fn unregister(&self, instance_id: Uuid) -> Result<()> {
        let url = format!("{}/api/v1/users/{}", self.server_url, instance_id);

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(&url)
            .body(AsyncBody::empty())?;

        let response = self.http_client.send(request).await?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(anyhow::anyhow!(
                "Failed to unregister: HTTP {}",
                response.status()
            ));
        }

        Ok(())
    }
}

impl UserInfo {
    pub fn to_active_session_info(&self) -> Vec<ActiveSessionInfo> {
        self.active_sessions.clone()
    }
}

impl SessionProtocol {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ssh" => Some(Self::Ssh),
            "telnet" => Some(Self::Telnet),
            _ => None,
        }
    }
}
