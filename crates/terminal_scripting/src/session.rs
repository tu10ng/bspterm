use anyhow::Result;
use gpui::{App, Entity, Global, WeakEntity};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use terminal::Terminal;
use uuid::Uuid;

use crate::protocol::SessionInfo;
use crate::tracking::{CommandId, OutputTracker, ReaderId};

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
    trackers: HashMap<Uuid, OutputTracker>,
}

pub struct TerminalRegistry;

impl TerminalRegistry {
    pub fn init(cx: &mut App) {
        cx.set_global(GlobalTerminalRegistry(Arc::new(RwLock::new(
            TerminalRegistryInner {
                sessions: HashMap::new(),
                focused_terminal_id: None,
                trackers: HashMap::new(),
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

    pub fn create_reader(terminal_id: Uuid, cx: &App) -> Option<ReaderId> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        if !inner.sessions.contains_key(&terminal_id) {
            return None;
        }

        let tracker = inner
            .trackers
            .entry(terminal_id)
            .or_insert_with(OutputTracker::new);

        Some(tracker.create_reader())
    }

    pub fn read_new(terminal_id: Uuid, reader_id: ReaderId, cx: &App) -> Option<(String, bool)> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        let tracker = inner.trackers.get_mut(&terminal_id)?;
        tracker.read_new(reader_id)
    }

    pub fn stop_reader(terminal_id: Uuid, reader_id: ReaderId, cx: &App) -> bool {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        if let Some(tracker) = inner.trackers.get_mut(&terminal_id) {
            tracker.stop_reader(reader_id)
        } else {
            false
        }
    }

    pub fn record_output(terminal_id: Uuid, content: String, cx: &App) {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        if let Some(tracker) = inner.trackers.get_mut(&terminal_id) {
            tracker.record_output(content);
        }
    }

    pub fn start_command(terminal_id: Uuid, command: String, cx: &App) -> Option<CommandId> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        if !inner.sessions.contains_key(&terminal_id) {
            return None;
        }

        let tracker = inner
            .trackers
            .entry(terminal_id)
            .or_insert_with(OutputTracker::new);

        Some(tracker.start_command(command))
    }

    pub fn complete_command(terminal_id: Uuid, command_id: CommandId, cx: &App) -> bool {
        let global = cx.global::<GlobalTerminalRegistry>();
        let mut inner = global.0.write();

        if let Some(tracker) = inner.trackers.get_mut(&terminal_id) {
            tracker.complete_command(command_id)
        } else {
            false
        }
    }

    pub fn get_command_output(terminal_id: Uuid, command_id: CommandId, cx: &App) -> Option<String> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();

        let tracker = inner.trackers.get(&terminal_id)?;
        tracker.get_command_output(command_id)
    }

    pub fn read_time_range(terminal_id: Uuid, start_ms: u64, end_ms: u64, cx: &App) -> Option<String> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();

        let tracker = inner.trackers.get(&terminal_id)?;
        Some(tracker.read_time_range(start_ms, end_ms))
    }

    pub fn get_tracker_elapsed_ms(terminal_id: Uuid, cx: &App) -> Option<u64> {
        let global = cx.global::<GlobalTerminalRegistry>();
        let inner = global.0.read();

        let tracker = inner.trackers.get(&terminal_id)?;
        Some(tracker.elapsed_ms())
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
