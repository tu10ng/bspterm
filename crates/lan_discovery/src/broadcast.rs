use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LAN_DISCOVERY_PORT: u16 = 53721;
pub const BROADCAST_INTERVAL_SECS: u64 = 5;
pub const USER_TIMEOUT_SECS: u64 = 30;

/// Protocol type for active sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionProtocol {
    Ssh,
    Telnet,
}

/// Information about an active terminal session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveSessionInfo {
    pub session_id: Uuid,
    pub host: String,
    pub port: u16,
    pub protocol: SessionProtocol,
}

impl ActiveSessionInfo {
    pub fn new(session_id: Uuid, host: String, port: u16, protocol: SessionProtocol) -> Self {
        Self {
            session_id,
            host,
            port,
            protocol,
        }
    }
}

/// A broadcast message containing user presence information.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPresenceBroadcast {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
    pub timestamp: u64,
}

impl UserPresenceBroadcast {
    pub fn new(
        employee_id: String,
        name: String,
        instance_id: Uuid,
        ip_addresses: Vec<IpAddr>,
        active_sessions: Vec<ActiveSessionInfo>,
    ) -> Self {
        Self {
            employee_id,
            name,
            instance_id,
            ip_addresses,
            active_sessions,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_presence_broadcast_serialization() {
        let broadcast = UserPresenceBroadcast::new(
            "12345".to_string(),
            "Zhang San".to_string(),
            Uuid::new_v4(),
            vec!["192.168.1.100".parse().unwrap()],
            vec![ActiveSessionInfo::new(
                Uuid::new_v4(),
                "192.168.1.1".to_string(),
                22,
                SessionProtocol::Ssh,
            )],
        );

        let bytes = broadcast.to_bytes().unwrap();
        let restored = UserPresenceBroadcast::from_bytes(&bytes).unwrap();

        assert_eq!(restored.employee_id, broadcast.employee_id);
        assert_eq!(restored.name, broadcast.name);
        assert_eq!(restored.instance_id, broadcast.instance_id);
        assert_eq!(restored.ip_addresses.len(), 1);
        assert_eq!(restored.active_sessions.len(), 1);
    }
}
