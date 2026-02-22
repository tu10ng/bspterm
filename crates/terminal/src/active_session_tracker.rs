use std::collections::HashMap;

use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use uuid::Uuid;

/// Information about an active terminal session.
#[derive(Clone, Debug)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub host: String,
    pub port: u16,
    pub protocol: SessionProtocolType,
}

/// Protocol type for sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionProtocolType {
    Ssh,
    Telnet,
}

/// Events emitted by the active session tracker.
#[derive(Clone, Debug)]
pub enum ActiveSessionTrackerEvent {
    SessionConnected(ActiveSession),
    SessionDisconnected(Uuid),
    SessionsChanged,
}

/// Global marker for cx.global access.
pub struct GlobalActiveSessionTracker(pub Entity<ActiveSessionTrackerEntity>);
impl Global for GlobalActiveSessionTracker {}

/// GPUI Entity for tracking active terminal sessions.
pub struct ActiveSessionTrackerEntity {
    sessions: HashMap<Uuid, ActiveSession>,
}

impl EventEmitter<ActiveSessionTrackerEvent> for ActiveSessionTrackerEntity {}

impl ActiveSessionTrackerEntity {
    /// Initialize global tracker on app startup.
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| Self {
            sessions: HashMap::new(),
        });

        cx.set_global(GlobalActiveSessionTracker(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalActiveSessionTracker>().0.clone()
    }

    /// Try to get global instance.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalActiveSessionTracker>()
            .map(|g| g.0.clone())
    }

    /// Register a new active session.
    pub fn register_session(&mut self, session: ActiveSession, cx: &mut Context<Self>) {
        let session_id = session.session_id;
        self.sessions.insert(session_id, session.clone());
        cx.emit(ActiveSessionTrackerEvent::SessionConnected(session));
        cx.emit(ActiveSessionTrackerEvent::SessionsChanged);
        cx.notify();
    }

    /// Unregister an active session.
    pub fn unregister_session(&mut self, session_id: Uuid, cx: &mut Context<Self>) {
        if self.sessions.remove(&session_id).is_some() {
            cx.emit(ActiveSessionTrackerEvent::SessionDisconnected(session_id));
            cx.emit(ActiveSessionTrackerEvent::SessionsChanged);
            cx.notify();
        }
    }

    /// Get all active sessions.
    pub fn sessions(&self) -> impl Iterator<Item = &ActiveSession> {
        self.sessions.values()
    }

    /// Get active sessions for a specific host/port.
    pub fn sessions_for_host(&self, host: &str, port: u16) -> Vec<&ActiveSession> {
        self.sessions
            .values()
            .filter(|s| s.host == host && s.port == port)
            .collect()
    }

    /// Get an active session by ID.
    pub fn get_session(&self, session_id: Uuid) -> Option<&ActiveSession> {
        self.sessions.get(&session_id)
    }

    /// Check if a session is active.
    pub fn is_session_active(&self, session_id: Uuid) -> bool {
        self.sessions.contains_key(&session_id)
    }

    /// Get all active sessions as a list.
    pub fn active_sessions_list(&self) -> Vec<ActiveSession> {
        self.sessions.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_session() {
        let session = ActiveSession {
            session_id: Uuid::new_v4(),
            host: "192.168.1.1".to_string(),
            port: 22,
            protocol: SessionProtocolType::Ssh,
        };

        assert_eq!(session.host, "192.168.1.1");
        assert_eq!(session.port, 22);
        assert_eq!(session.protocol, SessionProtocolType::Ssh);
    }
}
