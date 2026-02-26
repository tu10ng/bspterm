use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

pub const USER_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionProtocol {
    Ssh,
    Telnet,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveSessionInfo {
    pub session_id: Uuid,
    pub host: String,
    pub port: u16,
    pub protocol: SessionProtocol,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserRegistration {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
}

#[derive(Clone, Debug)]
pub struct RegisteredUser {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
    pub last_seen: Instant,
}

impl RegisteredUser {
    pub fn from_registration(reg: &UserRegistration) -> Self {
        Self {
            employee_id: reg.employee_id.clone(),
            name: reg.name.clone(),
            instance_id: reg.instance_id,
            ip_addresses: reg.ip_addresses.clone(),
            active_sessions: reg.active_sessions.clone(),
            last_seen: Instant::now(),
        }
    }

    pub fn update_from_registration(&mut self, reg: &UserRegistration) {
        self.employee_id = reg.employee_id.clone();
        self.name = reg.name.clone();
        self.ip_addresses = reg.ip_addresses.clone();
        self.active_sessions = reg.active_sessions.clone();
        self.last_seen = Instant::now();
    }

    pub fn is_expired(&self) -> bool {
        self.last_seen.elapsed() > Duration::from_secs(USER_TIMEOUT_SECS)
    }

    pub fn to_user_info(&self) -> UserInfo {
        UserInfo {
            employee_id: self.employee_id.clone(),
            name: self.name.clone(),
            instance_id: self.instance_id,
            ip_addresses: self.ip_addresses.clone(),
            active_sessions: self.active_sessions.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
}

#[derive(Clone)]
pub struct AppState {
    pub users: Arc<RwLock<HashMap<Uuid, RegisteredUser>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_user(&self, reg: UserRegistration) -> bool {
        let mut users = self.users.write().await;
        let instance_id = reg.instance_id;

        if let Some(user) = users.get_mut(&instance_id) {
            user.update_from_registration(&reg);
            false
        } else {
            users.insert(instance_id, RegisteredUser::from_registration(&reg));
            true
        }
    }

    pub async fn unregister_user(&self, instance_id: Uuid) -> bool {
        let mut users = self.users.write().await;
        users.remove(&instance_id).is_some()
    }

    pub async fn get_users(&self, exclude_instance_id: Option<Uuid>) -> Vec<UserInfo> {
        let users = self.users.read().await;
        users
            .values()
            .filter(|u| {
                if let Some(exclude_id) = exclude_instance_id {
                    u.instance_id != exclude_id
                } else {
                    true
                }
            })
            .filter(|u| !u.is_expired())
            .map(|u| u.to_user_info())
            .collect()
    }

    pub async fn cleanup_expired(&self) -> usize {
        let mut users = self.users.write().await;
        let expired: Vec<Uuid> = users
            .iter()
            .filter(|(_, user)| user.is_expired())
            .map(|(id, _)| *id)
            .collect();

        let count = expired.len();
        for id in expired {
            users.remove(&id);
        }
        count
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get_users() {
        let state = AppState::new();
        let instance_id = Uuid::new_v4();

        let reg = UserRegistration {
            employee_id: "12345".to_string(),
            name: "Test User".to_string(),
            instance_id,
            ip_addresses: vec!["192.168.1.100".parse().unwrap()],
            active_sessions: vec![],
        };

        let is_new = state.register_user(reg.clone()).await;
        assert!(is_new);

        let users = state.get_users(None).await;
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Test User");

        let is_new = state.register_user(reg).await;
        assert!(!is_new);
    }

    #[tokio::test]
    async fn test_unregister_user() {
        let state = AppState::new();
        let instance_id = Uuid::new_v4();

        let reg = UserRegistration {
            employee_id: "12345".to_string(),
            name: "Test User".to_string(),
            instance_id,
            ip_addresses: vec![],
            active_sessions: vec![],
        };

        state.register_user(reg).await;
        assert_eq!(state.get_users(None).await.len(), 1);

        let removed = state.unregister_user(instance_id).await;
        assert!(removed);
        assert_eq!(state.get_users(None).await.len(), 0);
    }

    #[tokio::test]
    async fn test_exclude_self() {
        let state = AppState::new();
        let my_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();

        state
            .register_user(UserRegistration {
                employee_id: "1".to_string(),
                name: "Me".to_string(),
                instance_id: my_id,
                ip_addresses: vec![],
                active_sessions: vec![],
            })
            .await;

        state
            .register_user(UserRegistration {
                employee_id: "2".to_string(),
                name: "Other".to_string(),
                instance_id: other_id,
                ip_addresses: vec![],
                active_sessions: vec![],
            })
            .await;

        let users = state.get_users(Some(my_id)).await;
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Other");
    }
}
