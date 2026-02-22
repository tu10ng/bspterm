use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::channel::mpsc;
use futures::StreamExt;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use local_user::LocalUserStoreEntity;
use uuid::Uuid;

use crate::broadcast::{
    ActiveSessionInfo, UserPresenceBroadcast, BROADCAST_INTERVAL_SECS, LAN_DISCOVERY_PORT,
    USER_TIMEOUT_SECS,
};

/// A discovered user on the LAN.
#[derive(Clone, Debug)]
pub struct DiscoveredUser {
    pub employee_id: String,
    pub name: String,
    pub instance_id: Uuid,
    pub ip_addresses: Vec<IpAddr>,
    pub active_sessions: Vec<ActiveSessionInfo>,
    pub last_seen: Instant,
}

impl DiscoveredUser {
    pub fn from_broadcast(broadcast: &UserPresenceBroadcast) -> Self {
        Self {
            employee_id: broadcast.employee_id.clone(),
            name: broadcast.name.clone(),
            instance_id: broadcast.instance_id,
            ip_addresses: broadcast.ip_addresses.clone(),
            active_sessions: broadcast.active_sessions.clone(),
            last_seen: Instant::now(),
        }
    }

    pub fn update_from_broadcast(&mut self, broadcast: &UserPresenceBroadcast) {
        self.ip_addresses = broadcast.ip_addresses.clone();
        self.active_sessions = broadcast.active_sessions.clone();
        self.last_seen = Instant::now();
    }

    pub fn is_expired(&self) -> bool {
        self.last_seen.elapsed() > Duration::from_secs(USER_TIMEOUT_SECS)
    }

    pub fn initials(&self) -> String {
        self.name.chars().take(2).collect::<String>().to_uppercase()
    }
}

/// Events emitted by the LAN discovery service.
#[derive(Clone, Debug)]
pub enum LanDiscoveryEvent {
    UserDiscovered(Uuid),
    UserUpdated(Uuid),
    UserOffline(Uuid),
    SessionsChanged,
}

/// Global marker for cx.global access.
pub struct GlobalLanDiscovery(pub Entity<LanDiscoveryEntity>);
impl Global for GlobalLanDiscovery {}

/// GPUI Entity for LAN discovery.
pub struct LanDiscoveryEntity {
    instance_id: Uuid,
    users: HashMap<Uuid, DiscoveredUser>,
    active_sessions: Vec<ActiveSessionInfo>,
    _broadcast_task: Option<Task<()>>,
    _listener_task: Option<Task<()>>,
    _cleanup_task: Option<Task<()>>,
}

impl EventEmitter<LanDiscoveryEvent> for LanDiscoveryEntity {}

impl LanDiscoveryEntity {
    /// Initialize global LAN discovery on app startup.
    pub fn init(cx: &mut App) {
        let instance_id = Uuid::new_v4();

        let entity = cx.new(|_| Self {
            instance_id,
            users: HashMap::new(),
            active_sessions: Vec::new(),
            _broadcast_task: None,
            _listener_task: None,
            _cleanup_task: None,
        });

        cx.set_global(GlobalLanDiscovery(entity.clone()));

        entity.update(cx, |this, cx| {
            this.start(cx);
        });
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalLanDiscovery>().0.clone()
    }

    /// Try to get global instance.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalLanDiscovery>().map(|g| g.0.clone())
    }

    /// Get all discovered users.
    pub fn users(&self) -> impl Iterator<Item = &DiscoveredUser> {
        self.users.values()
    }

    /// Get users for a specific session by session_id.
    pub fn users_for_session(&self, session_id: Uuid) -> Vec<&DiscoveredUser> {
        self.users
            .values()
            .filter(|user| {
                user.active_sessions
                    .iter()
                    .any(|s| s.session_id == session_id)
            })
            .collect()
    }

    /// Get users for a specific host/port combination.
    pub fn users_for_host(&self, host: &str, port: u16) -> Vec<&DiscoveredUser> {
        self.users
            .values()
            .filter(|user| {
                user.active_sessions
                    .iter()
                    .any(|s| s.host == host && s.port == port)
            })
            .collect()
    }

    /// Register an active session.
    pub fn register_session(&mut self, session: ActiveSessionInfo, cx: &mut Context<Self>) {
        if !self
            .active_sessions
            .iter()
            .any(|s| s.session_id == session.session_id)
        {
            self.active_sessions.push(session);
            cx.emit(LanDiscoveryEvent::SessionsChanged);
        }
    }

    /// Unregister an active session.
    pub fn unregister_session(&mut self, session_id: Uuid, cx: &mut Context<Self>) {
        if let Some(pos) = self
            .active_sessions
            .iter()
            .position(|s| s.session_id == session_id)
        {
            self.active_sessions.remove(pos);
            cx.emit(LanDiscoveryEvent::SessionsChanged);
        }
    }

    /// Get active sessions.
    pub fn active_sessions(&self) -> &[ActiveSessionInfo] {
        &self.active_sessions
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let socket = match UdpSocket::bind(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            LAN_DISCOVERY_PORT,
        )) {
            Ok(s) => {
                if let Err(e) = s.set_broadcast(true) {
                    log::error!("Failed to set broadcast on socket: {}", e);
                }
                if let Err(e) = s.set_nonblocking(true) {
                    log::error!("Failed to set non-blocking on socket: {}", e);
                }
                Arc::new(s)
            }
            Err(e) => {
                log::error!("Failed to bind UDP socket for LAN discovery: {}", e);
                return;
            }
        };

        self.start_broadcaster(socket.clone(), cx);
        self.start_listener(socket, cx);
        self.start_cleanup(cx);
    }

    fn start_broadcaster(&mut self, socket: Arc<UdpSocket>, cx: &mut Context<Self>) {
        let instance_id = self.instance_id;

        self._broadcast_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(BROADCAST_INTERVAL_SECS))
                    .await;

                let broadcast_result: Option<UserPresenceBroadcast> = cx.update(|cx| {
                    let user_store = LocalUserStoreEntity::try_global(cx)?;
                    let user_store = user_store.read(cx);
                    let profile = user_store.profile()?;

                    let active_sessions = this
                        .upgrade()
                        .map(|entity| entity.read(cx).active_sessions.clone())
                        .unwrap_or_default();

                    let ip_addresses = user_store.ip_addresses();

                    Some(UserPresenceBroadcast::new(
                        profile.employee_id.clone(),
                        profile.name.clone(),
                        instance_id,
                        ip_addresses,
                        active_sessions,
                    ))
                });

                if let Some(broadcast) = broadcast_result {
                    if let Ok(bytes) = broadcast.to_bytes() {
                        let addr =
                            SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), LAN_DISCOVERY_PORT);
                        if let Err(e) = socket.send_to(&bytes, addr) {
                            log::trace!("Failed to send broadcast: {}", e);
                        }
                    }
                }
            }
        }));
    }

    fn start_listener(&mut self, socket: Arc<UdpSocket>, cx: &mut Context<Self>) {
        let instance_id = self.instance_id;
        let (tx, mut rx) = mpsc::unbounded::<UserPresenceBroadcast>();

        std::thread::spawn(move || {
            let mut buf = [0u8; 65535];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, _addr)) => {
                        if let Ok(broadcast) = UserPresenceBroadcast::from_bytes(&buf[..len]) {
                            if broadcast.instance_id != instance_id {
                                let _ = tx.unbounded_send(broadcast);
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::trace!("Error receiving broadcast: {}", e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        self._listener_task = Some(cx.spawn(async move |this, cx| {
            while let Some(broadcast) = rx.next().await {
                let _ = cx.update(|cx| {
                    if let Some(entity) = this.upgrade() {
                        entity.update(cx, |this, cx| {
                            this.handle_broadcast(broadcast, cx);
                        });
                    }
                });
            }
        }));
    }

    fn start_cleanup(&mut self, cx: &mut Context<Self>) {
        self._cleanup_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(USER_TIMEOUT_SECS / 2))
                    .await;

                let _ = cx.update(|cx| {
                    if let Some(entity) = this.upgrade() {
                        entity.update(cx, |this, cx| {
                            this.cleanup_expired_users(cx);
                        });
                    }
                });
            }
        }));
    }

    fn handle_broadcast(&mut self, broadcast: UserPresenceBroadcast, cx: &mut Context<Self>) {
        let instance_id = broadcast.instance_id;

        if let Some(user) = self.users.get_mut(&instance_id) {
            user.update_from_broadcast(&broadcast);
            cx.emit(LanDiscoveryEvent::UserUpdated(instance_id));
        } else {
            let user = DiscoveredUser::from_broadcast(&broadcast);
            self.users.insert(instance_id, user);
            cx.emit(LanDiscoveryEvent::UserDiscovered(instance_id));
        }

        cx.notify();
    }

    fn cleanup_expired_users(&mut self, cx: &mut Context<Self>) {
        let expired: Vec<Uuid> = self
            .users
            .iter()
            .filter(|(_, user)| user.is_expired())
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            self.users.remove(&id);
            cx.emit(LanDiscoveryEvent::UserOffline(id));
        }

        if !self.users.is_empty() {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_user_initials() {
        let broadcast = UserPresenceBroadcast::new(
            "12345".to_string(),
            "Zhang San".to_string(),
            Uuid::new_v4(),
            vec![],
            vec![],
        );
        let user = DiscoveredUser::from_broadcast(&broadcast);
        assert_eq!(user.initials(), "ZH");
    }

    #[test]
    fn test_discovered_user_expiry() {
        let mut broadcast = UserPresenceBroadcast::new(
            "12345".to_string(),
            "Zhang San".to_string(),
            Uuid::new_v4(),
            vec![],
            vec![],
        );
        broadcast.timestamp = 0;
        let user = DiscoveredUser::from_broadcast(&broadcast);
        assert!(!user.is_expired());
    }
}
