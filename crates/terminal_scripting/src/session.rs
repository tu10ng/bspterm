use anyhow::Result;
use gpui::{App, Entity, Global, WeakEntity};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use terminal::Terminal;
use uuid::Uuid;

use crate::protocol::SessionInfo;

pub struct TerminalSession {
    pub id: Uuid,
    pub name: String,
    pub terminal: WeakEntity<Terminal>,
}

impl TerminalSession {
    pub fn new(name: String, terminal: WeakEntity<Terminal>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            terminal,
        }
    }

    pub fn to_info(&self, cx: &App) -> Option<SessionInfo> {
        let terminal = self.terminal.upgrade()?;
        let connected = terminal.read(cx).is_connected();
        Some(SessionInfo {
            id: self.id.to_string(),
            name: self.name.clone(),
            session_type: self.session_type(cx),
            connected,
        })
    }

    fn session_type(&self, cx: &App) -> String {
        if let Some(terminal) = self.terminal.upgrade() {
            let term = terminal.read(cx);
            if term.is_remote() {
                "remote".to_string()
            } else {
                "local".to_string()
            }
        } else {
            "unknown".to_string()
        }
    }
}

struct GlobalTerminalRegistry(Arc<RwLock<TerminalRegistryInner>>);

impl Global for GlobalTerminalRegistry {}

struct TerminalRegistryInner {
    sessions: HashMap<Uuid, TerminalSession>,
    focused_terminal_id: Option<Uuid>,
}

pub struct TerminalRegistry;

impl TerminalRegistry {
    pub fn init(cx: &mut App) {
        cx.set_global(GlobalTerminalRegistry(Arc::new(RwLock::new(
            TerminalRegistryInner {
                sessions: HashMap::new(),
                focused_terminal_id: None,
            },
        ))));
    }

    pub fn register(terminal: &Entity<Terminal>, name: String, cx: &App) -> Uuid {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();
        let session = TerminalSession::new(name, terminal.downgrade());
        let id = session.id;
        inner.sessions.insert(id, session);
        id
    }

    pub fn unregister(id: Uuid, cx: &App) {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();
        inner.sessions.remove(&id);
        if inner.focused_terminal_id == Some(id) {
            inner.focused_terminal_id = None;
        }
    }

    pub fn set_focused(id: Option<Uuid>, cx: &App) {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();
        inner.focused_terminal_id = id;
    }

    pub fn focused_id(cx: &App) -> Option<Uuid> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();
        inner.focused_terminal_id
    }

    pub fn list(cx: &App) -> Vec<SessionInfo> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();
        inner
            .sessions
            .values()
            .filter_map(|session| session.to_info(cx))
            .collect()
    }

    pub fn get_terminal(id: Uuid, cx: &App) -> Option<Entity<Terminal>> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();
        inner
            .sessions
            .get(&id)
            .and_then(|session| session.terminal.upgrade())
    }

    pub fn get_by_id_str(id_str: &str, cx: &App) -> Result<Entity<Terminal>> {
        let id = Uuid::parse_str(id_str)
            .map_err(|_| anyhow::anyhow!("Invalid terminal ID format: {}", id_str))?;
        Self::get_terminal(id, cx)
            .ok_or_else(|| anyhow::anyhow!("Terminal not found: {}", id_str))
    }

    pub fn get_focused(cx: &App) -> Option<(Uuid, Entity<Terminal>)> {
        let id = Self::focused_id(cx)?;
        let terminal = Self::get_terminal(id, cx)?;
        Some((id, terminal))
    }
}

pub trait TerminalExt {
    fn is_connected(&self) -> bool;
    fn is_remote(&self) -> bool;
}

impl TerminalExt for Terminal {
    fn is_connected(&self) -> bool {
        !self.is_disconnected()
    }

    fn is_remote(&self) -> bool {
        self.connection_info().is_some()
    }
}
