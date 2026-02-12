use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::ssh::{SshAuthConfig, SshConfig};

/// A node in the session tree, either a group or a session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionNode {
    Group(SessionGroup),
    Session(SessionConfig),
}

impl SessionNode {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Group(g) => g.id,
            Self::Session(s) => s.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Group(g) => &g.name,
            Self::Session(s) => &s.name,
        }
    }

    pub fn as_group_mut(&mut self) -> Option<&mut SessionGroup> {
        match self {
            Self::Group(g) => Some(g),
            _ => None,
        }
    }
}

fn default_expanded() -> bool {
    true
}

/// A group of sessions, can contain other groups or sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionGroup {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_expanded")]
    pub expanded: bool,
    pub children: Vec<SessionNode>,
}

impl SessionGroup {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            expanded: true,
            children: Vec::new(),
        }
    }
}

/// Configuration for a saved session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionConfig {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub protocol: ProtocolConfig,
}

impl SessionConfig {
    pub fn new_ssh(name: impl Into<String>, ssh_config: SshSessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            tags: Vec::new(),
            protocol: ProtocolConfig::Ssh(ssh_config),
        }
    }

    pub fn new_telnet(name: impl Into<String>, telnet_config: TelnetSessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            tags: Vec::new(),
            protocol: ProtocolConfig::Telnet(telnet_config),
        }
    }
}

/// Protocol-specific configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "protocol")]
pub enum ProtocolConfig {
    Ssh(SshSessionConfig),
    Telnet(TelnetSessionConfig),
}

/// SSH session configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshSessionConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub auth: AuthMethod,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub keepalive_interval_secs: Option<u64>,
    pub initial_command: Option<String>,
}

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            username: None,
            auth: AuthMethod::Interactive,
            env: HashMap::new(),
            keepalive_interval_secs: Some(30),
            initial_command: None,
        }
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    pub fn with_auth(mut self, auth: AuthMethod) -> Self {
        self.auth = auth;
        self
    }
}

/// Authentication method for sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum AuthMethod {
    Interactive,
    Password { password: String },
    PrivateKey { path: PathBuf, passphrase: Option<String> },
    Agent,
}

/// Telnet session configuration (placeholder for future implementation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelnetSessionConfig {
    pub host: String,
    pub port: u16,
    pub encoding: Option<String>,
}

impl TelnetSessionConfig {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            encoding: None,
        }
    }
}

impl From<&SshSessionConfig> for SshConfig {
    fn from(config: &SshSessionConfig) -> Self {
        let mut ssh_config = SshConfig::new(&config.host, config.port);
        if let Some(username) = &config.username {
            ssh_config = ssh_config.with_username(username);
        }
        ssh_config = ssh_config.with_auth((&config.auth).into());
        ssh_config = ssh_config.with_env(config.env.clone().into_iter().collect());
        if let Some(secs) = config.keepalive_interval_secs {
            ssh_config = ssh_config.with_keepalive(Duration::from_secs(secs));
        }
        if let Some(cmd) = &config.initial_command {
            ssh_config = ssh_config.with_initial_command(cmd);
        }
        ssh_config
    }
}

impl From<&AuthMethod> for SshAuthConfig {
    fn from(method: &AuthMethod) -> Self {
        match method {
            AuthMethod::Interactive => SshAuthConfig::Auto,
            AuthMethod::Password { password } => SshAuthConfig::Password(password.clone()),
            AuthMethod::PrivateKey { path, passphrase } => SshAuthConfig::PrivateKey {
                path: path.clone(),
                passphrase: passphrase.clone(),
            },
            AuthMethod::Agent => SshAuthConfig::Auto,
        }
    }
}

impl From<&SshConfig> for SshSessionConfig {
    fn from(config: &SshConfig) -> Self {
        Self {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            auth: (&config.auth).into(),
            env: config.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            keepalive_interval_secs: config.keepalive_interval.map(|d| d.as_secs()),
            initial_command: config.initial_command.clone(),
        }
    }
}

impl From<&SshAuthConfig> for AuthMethod {
    fn from(config: &SshAuthConfig) -> Self {
        match config {
            SshAuthConfig::Auto => AuthMethod::Interactive,
            SshAuthConfig::Password(password) => AuthMethod::Password { password: password.clone() },
            SshAuthConfig::PrivateKey { path, passphrase } => AuthMethod::PrivateKey {
                path: path.clone(),
                passphrase: passphrase.clone(),
            },
        }
    }
}

/// The session store containing all saved sessions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionStore {
    pub version: u32,
    pub root: Vec<SessionNode>,
}

impl SessionStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            root: Vec::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn add_node(&mut self, node: SessionNode, parent_id: Option<Uuid>) {
        match parent_id {
            None => self.root.push(node),
            Some(pid) => {
                let node_clone = node;
                Self::modify_node_recursive(&mut self.root, pid, |parent| {
                    if let Some(group) = parent.as_group_mut() {
                        group.children.push(node_clone.clone());
                    }
                });
            }
        }
    }

    pub fn remove_node(&mut self, id: Uuid) -> bool {
        Self::remove_node_recursive(&mut self.root, id)
    }

    pub fn find_node(&self, id: Uuid) -> Option<&SessionNode> {
        Self::find_node_recursive(&self.root, id)
    }

    pub fn find_node_mut(&mut self, id: Uuid) -> Option<&mut SessionNode> {
        Self::find_node_mut_recursive(&mut self.root, id)
    }

    fn modify_node_recursive<F>(nodes: &mut Vec<SessionNode>, id: Uuid, f: F) -> bool
    where
        F: Fn(&mut SessionNode) + Clone,
    {
        for node in nodes.iter_mut() {
            if node.id() == id {
                f(node);
                return true;
            }
            if let Some(group) = node.as_group_mut() {
                if Self::modify_node_recursive(&mut group.children, id, f.clone()) {
                    return true;
                }
            }
        }
        false
    }

    fn remove_node_recursive(nodes: &mut Vec<SessionNode>, id: Uuid) -> bool {
        if let Some(pos) = nodes.iter().position(|n| n.id() == id) {
            nodes.remove(pos);
            return true;
        }
        for node in nodes.iter_mut() {
            if let Some(group) = node.as_group_mut() {
                if Self::remove_node_recursive(&mut group.children, id) {
                    return true;
                }
            }
        }
        false
    }

    fn find_node_recursive(nodes: &[SessionNode], id: Uuid) -> Option<&SessionNode> {
        for node in nodes {
            if node.id() == id {
                return Some(node);
            }
            if let SessionNode::Group(group) = node {
                if let Some(found) = Self::find_node_recursive(&group.children, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn find_node_mut_recursive(nodes: &mut [SessionNode], id: Uuid) -> Option<&mut SessionNode> {
        for node in nodes {
            if node.id() == id {
                return Some(node);
            }
            if let SessionNode::Group(group) = node {
                if let Some(found) = Self::find_node_mut_recursive(&mut group.children, id) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// Events emitted by the session store for UI subscription.
#[derive(Clone, Debug)]
pub enum SessionStoreEvent {
    Changed,
    SessionAdded(Uuid),
    SessionRemoved(Uuid),
}

/// Global marker for cx.global access.
pub struct GlobalSessionStore(pub Entity<SessionStoreEntity>);
impl Global for GlobalSessionStore {}

/// GPUI Entity wrapping SessionStore.
pub struct SessionStoreEntity {
    store: SessionStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<SessionStoreEvent> for SessionStoreEntity {}

impl SessionStoreEntity {
    /// Initialize global session store on app startup.
    pub fn init(cx: &mut App) {
        let store = SessionStore::load_from_file(paths::sessions_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load sessions: {}", err);
                SessionStore::new()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalSessionStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalSessionStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalSessionStore>().map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    /// Add a session and trigger save.
    pub fn add_session(
        &mut self,
        config: SessionConfig,
        parent_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) {
        let id = config.id;
        self.store.add_node(SessionNode::Session(config), parent_id);
        self.schedule_save(cx);
        cx.emit(SessionStoreEvent::SessionAdded(id));
        cx.notify();
    }

    /// Add a group and trigger save.
    pub fn add_group(
        &mut self,
        group: SessionGroup,
        parent_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) {
        self.store.add_node(SessionNode::Group(group), parent_id);
        self.schedule_save(cx);
        cx.emit(SessionStoreEvent::Changed);
        cx.notify();
    }

    /// Remove node and trigger save.
    pub fn remove_node(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_node(id) {
            self.schedule_save(cx);
            cx.emit(SessionStoreEvent::SessionRemoved(id));
            cx.notify();
        }
    }

    /// Update a session and trigger save.
    pub fn update_session(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut SessionConfig),
        cx: &mut Context<Self>,
    ) {
        if let Some(SessionNode::Session(config)) = self.store.find_node_mut(id) {
            update_fn(config);
            self.schedule_save(cx);
            cx.emit(SessionStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle group expanded state.
    pub fn toggle_group_expanded(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(SessionNode::Group(group)) = self.store.find_node_mut(id) {
            group.expanded = !group.expanded;
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::sessions_file()) {
                log::error!("Failed to save sessions: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_store_serialization_roundtrip() {
        let mut store = SessionStore::new();

        let ssh_config = SshSessionConfig::new("example.com", 22)
            .with_username("admin")
            .with_auth(AuthMethod::Password { password: "secret".into() });

        let session = SessionConfig::new_ssh("My Server", ssh_config);
        store.add_node(SessionNode::Session(session), None);

        let mut group = SessionGroup::new("Production");
        let nested_session = SessionConfig::new_ssh(
            "DB Server",
            SshSessionConfig::new("db.example.com", 22),
        );
        group.children.push(SessionNode::Session(nested_session));
        store.add_node(SessionNode::Group(group), None);

        let json = serde_json::to_string_pretty(&store).expect("serialize");
        let restored: SessionStore = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.version, store.version);
        assert_eq!(restored.root.len(), 2);
    }

    #[test]
    fn test_add_to_root() {
        let mut store = SessionStore::new();
        let session = SessionConfig::new_ssh("Test", SshSessionConfig::new("host", 22));
        let id = session.id;

        store.add_node(SessionNode::Session(session), None);

        assert!(store.find_node(id).is_some());
    }

    #[test]
    fn test_add_to_group() {
        let mut store = SessionStore::new();
        let group = SessionGroup::new("My Group");
        let group_id = group.id;
        store.add_node(SessionNode::Group(group), None);

        let session = SessionConfig::new_ssh("Test", SshSessionConfig::new("host", 22));
        let session_id = session.id;
        store.add_node(SessionNode::Session(session), Some(group_id));

        assert!(store.find_node(session_id).is_some());
        if let Some(SessionNode::Group(g)) = store.find_node(group_id) {
            assert_eq!(g.children.len(), 1);
        } else {
            panic!("Expected group");
        }
    }

    #[test]
    fn test_remove_from_root() {
        let mut store = SessionStore::new();
        let session = SessionConfig::new_ssh("Test", SshSessionConfig::new("host", 22));
        let id = session.id;
        store.add_node(SessionNode::Session(session), None);

        assert!(store.remove_node(id));
        assert!(store.find_node(id).is_none());
    }

    #[test]
    fn test_remove_from_nested_group() {
        let mut store = SessionStore::new();

        let mut outer_group = SessionGroup::new("Outer");
        let inner_group = SessionGroup::new("Inner");
        let inner_id = inner_group.id;
        outer_group.children.push(SessionNode::Group(inner_group));
        store.add_node(SessionNode::Group(outer_group), None);

        let session = SessionConfig::new_ssh("Test", SshSessionConfig::new("host", 22));
        let session_id = session.id;
        store.add_node(SessionNode::Session(session), Some(inner_id));

        assert!(store.find_node(session_id).is_some());
        assert!(store.remove_node(session_id));
        assert!(store.find_node(session_id).is_none());
    }

    #[test]
    fn test_ssh_config_conversion() {
        let session_config = SshSessionConfig {
            host: "example.com".into(),
            port: 22,
            username: Some("user".into()),
            auth: AuthMethod::Password { password: "pass".into() },
            env: [("TERM".into(), "xterm".into())].into_iter().collect(),
            keepalive_interval_secs: Some(60),
            initial_command: Some("htop".into()),
        };

        let ssh_config: SshConfig = (&session_config).into();

        assert_eq!(ssh_config.host, "example.com");
        assert_eq!(ssh_config.port, 22);
        assert_eq!(ssh_config.username, Some("user".into()));
        assert!(matches!(ssh_config.auth, SshAuthConfig::Password(_)));
        assert_eq!(ssh_config.keepalive_interval, Some(Duration::from_secs(60)));
        assert_eq!(ssh_config.initial_command, Some("htop".into()));
    }

    #[test]
    fn test_auth_method_conversion_roundtrip() {
        let methods = vec![
            AuthMethod::Interactive,
            AuthMethod::Password { password: "secret".into() },
            AuthMethod::PrivateKey {
                path: PathBuf::from("/home/user/.ssh/id_rsa"),
                passphrase: Some("phrase".into()),
            },
            AuthMethod::Agent,
        ];

        for method in methods {
            let ssh_auth: SshAuthConfig = (&method).into();
            let back: AuthMethod = (&ssh_auth).into();

            match (&method, &back) {
                (AuthMethod::Interactive, AuthMethod::Interactive) => {}
                (AuthMethod::Agent, AuthMethod::Interactive) => {}
                (AuthMethod::Password { password: p1 }, AuthMethod::Password { password: p2 }) => {
                    assert_eq!(p1, p2);
                }
                (
                    AuthMethod::PrivateKey { path: p1, passphrase: pp1 },
                    AuthMethod::PrivateKey { path: p2, passphrase: pp2 },
                ) => {
                    assert_eq!(p1, p2);
                    assert_eq!(pp1, pp2);
                }
                _ => panic!("Conversion mismatch"),
            }
        }
    }

    #[test]
    fn test_telnet_config() {
        let config = TelnetSessionConfig::new("legacy.host.com", 23);
        let session = SessionConfig::new_telnet("Legacy System", config);

        let json = serde_json::to_string(&session).expect("serialize");
        let restored: SessionConfig = serde_json::from_str(&json).expect("deserialize");

        match restored.protocol {
            ProtocolConfig::Telnet(t) => {
                assert_eq!(t.host, "legacy.host.com");
                assert_eq!(t.port, 23);
            }
            _ => panic!("Expected telnet config"),
        }
    }
}
