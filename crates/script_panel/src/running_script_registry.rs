use std::time::Instant;

use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSource {
    Panel,
    ButtonBar,
    Function,
    Shortcut,
}

pub struct RunningScriptEntry {
    pub id: Uuid,
    pub script_name: String,
    pub source: ScriptSource,
    pub started_at: Instant,
    stop_fn: Option<Box<dyn FnOnce(&mut App) + 'static>>,
}

pub enum RunningScriptEvent {
    Changed,
}

pub struct RunningScriptRegistry {
    entries: Vec<RunningScriptEntry>,
}

pub struct GlobalRunningScriptRegistry(pub Entity<RunningScriptRegistry>);
impl Global for GlobalRunningScriptRegistry {}

impl EventEmitter<RunningScriptEvent> for RunningScriptRegistry {}

impl RunningScriptRegistry {
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_cx| Self {
            entries: Vec::new(),
        });
        cx.set_global(GlobalRunningScriptRegistry(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRunningScriptRegistry>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalRunningScriptRegistry>()
            .map(|g| g.0.clone())
    }

    pub fn register(
        &mut self,
        script_name: String,
        source: ScriptSource,
        stop_fn: impl FnOnce(&mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.entries.push(RunningScriptEntry {
            id,
            script_name,
            source,
            started_at: Instant::now(),
            stop_fn: Some(Box::new(stop_fn)),
        });
        cx.emit(RunningScriptEvent::Changed);
        cx.notify();
        id
    }

    pub fn unregister(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            self.entries.remove(pos);
            cx.emit(RunningScriptEvent::Changed);
            cx.notify();
        }
    }

    pub fn stop(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            let mut entry = self.entries.remove(pos);
            if let Some(stop_fn) = entry.stop_fn.take() {
                stop_fn(cx);
            }
            cx.emit(RunningScriptEvent::Changed);
            cx.notify();
        }
    }

    pub fn entries(&self) -> &[RunningScriptEntry] {
        &self.entries
    }
}
