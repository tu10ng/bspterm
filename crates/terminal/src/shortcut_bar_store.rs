use std::fs;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

/// Configuration for a single shortcut entry in the shortcut bar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShortcutEntry {
    pub id: Uuid,
    pub action_type: String,
    #[serde(default)]
    pub keybinding: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ShortcutEntry {
    pub fn new(
        action_type: impl Into<String>,
        keybinding: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: action_type.into(),
            keybinding: keybinding.into(),
            label: label.into(),
            enabled: true,
        }
    }

    pub fn new_disabled(
        action_type: impl Into<String>,
        keybinding: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: action_type.into(),
            keybinding: keybinding.into(),
            label: label.into(),
            enabled: false,
        }
    }
}

/// Predefined shortcut definitions for the Terminal context.
pub struct PredefinedShortcut {
    pub action_type: &'static str,
    pub keybinding: &'static str,
    pub label: &'static str,
    pub default_enabled: bool,
}

pub const PREDEFINED_SHORTCUTS: &[PredefinedShortcut] = &[
    PredefinedShortcut {
        action_type: "terminal::Copy",
        keybinding: "ctrl-shift-c",
        label: "复制",
        default_enabled: true,
    },
    PredefinedShortcut {
        action_type: "terminal::Paste",
        keybinding: "ctrl-shift-v",
        label: "粘贴",
        default_enabled: true,
    },
    PredefinedShortcut {
        action_type: "terminal::Clear",
        keybinding: "ctrl-l",
        label: "清屏",
        default_enabled: true,
    },
    PredefinedShortcut {
        action_type: "terminal::ClearScrollback",
        keybinding: "ctrl-shift-l",
        label: "清除滚动",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ScrollPageUp",
        keybinding: "shift-pageup",
        label: "上翻页",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ScrollPageDown",
        keybinding: "shift-pagedown",
        label: "下翻页",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ScrollToTop",
        keybinding: "ctrl-home",
        label: "到顶部",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ScrollToBottom",
        keybinding: "ctrl-end",
        label: "到底部",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "editor::SelectAll",
        keybinding: "ctrl-shift-a",
        label: "全选",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ToggleViMode",
        keybinding: "ctrl-shift-space",
        label: "Vi模式",
        default_enabled: false,
    },
    PredefinedShortcut {
        action_type: "terminal::ReconnectTerminal",
        keybinding: "ctrl-shift-r",
        label: "重连",
        default_enabled: true,
    },
    PredefinedShortcut {
        action_type: "terminal::DisconnectTerminal",
        keybinding: "ctrl-shift-d",
        label: "断开",
        default_enabled: true,
    },
];

/// The shortcut bar store containing all shortcut configurations.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShortcutBarStore {
    pub version: u32,
    #[serde(default)]
    pub shortcuts: Vec<ShortcutEntry>,
    #[serde(default = "default_true")]
    pub show_shortcut_bar: bool,
}

impl ShortcutBarStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            shortcuts: Vec::new(),
            show_shortcut_bar: true,
        }
    }

    pub fn with_defaults() -> Self {
        let shortcuts = PREDEFINED_SHORTCUTS
            .iter()
            .map(|preset| {
                if preset.default_enabled {
                    ShortcutEntry::new(preset.action_type, preset.keybinding, preset.label)
                } else {
                    ShortcutEntry::new_disabled(preset.action_type, preset.keybinding, preset.label)
                }
            })
            .collect();

        Self {
            version: Self::CURRENT_VERSION,
            shortcuts,
            show_shortcut_bar: true,
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::with_defaults());
        }
        let content = fs::read_to_string(path)?;
        let store: Self = serde_json::from_str(&content)?;
        Ok(store.ensure_all_predefined())
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    fn ensure_all_predefined(mut self) -> Self {
        for preset in PREDEFINED_SHORTCUTS {
            let exists = self
                .shortcuts
                .iter()
                .any(|s| s.action_type == preset.action_type);
            if !exists {
                let entry = if preset.default_enabled {
                    ShortcutEntry::new(preset.action_type, preset.keybinding, preset.label)
                } else {
                    ShortcutEntry::new_disabled(preset.action_type, preset.keybinding, preset.label)
                };
                self.shortcuts.push(entry);
            }
        }
        self
    }

    pub fn add_shortcut(&mut self, entry: ShortcutEntry) {
        self.shortcuts.push(entry);
    }

    pub fn remove_shortcut(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.shortcuts.iter().position(|s| s.id == id) {
            self.shortcuts.remove(pos);
            return true;
        }
        false
    }

    pub fn find_shortcut(&self, id: Uuid) -> Option<&ShortcutEntry> {
        self.shortcuts.iter().find(|s| s.id == id)
    }

    pub fn find_shortcut_mut(&mut self, id: Uuid) -> Option<&mut ShortcutEntry> {
        self.shortcuts.iter_mut().find(|s| s.id == id)
    }

    pub fn find_by_action_type(&self, action_type: &str) -> Option<&ShortcutEntry> {
        self.shortcuts.iter().find(|s| s.action_type == action_type)
    }

    pub fn enabled_shortcuts(&self) -> Vec<&ShortcutEntry> {
        self.shortcuts.iter().filter(|s| s.enabled).collect()
    }

    pub fn disabled_shortcuts(&self) -> Vec<&ShortcutEntry> {
        self.shortcuts.iter().filter(|s| !s.enabled).collect()
    }
}

/// Events emitted by the shortcut bar store for UI subscription.
#[derive(Clone, Debug)]
pub enum ShortcutBarStoreEvent {
    Changed,
    ShortcutAdded(Uuid),
    ShortcutRemoved(Uuid),
}

/// Global marker for cx.global access.
pub struct GlobalShortcutBarStore(pub Entity<ShortcutBarStoreEntity>);
impl Global for GlobalShortcutBarStore {}

/// GPUI Entity wrapping ShortcutBarStore.
pub struct ShortcutBarStoreEntity {
    store: ShortcutBarStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<ShortcutBarStoreEvent> for ShortcutBarStoreEntity {}

impl ShortcutBarStoreEntity {
    /// Initialize global shortcut bar store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalShortcutBarStore>().is_some() {
            return;
        }

        let store = ShortcutBarStore::load_from_file(paths::shortcut_bar_file()).unwrap_or_else(
            |err| {
                log::error!("Failed to load shortcut bar config: {}", err);
                ShortcutBarStore::with_defaults()
            },
        );

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalShortcutBarStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalShortcutBarStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalShortcutBarStore>()
            .map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &ShortcutBarStore {
        &self.store
    }

    /// Get whether shortcut bar is visible.
    pub fn show_shortcut_bar(&self) -> bool {
        self.store.show_shortcut_bar
    }

    /// Toggle shortcut bar visibility.
    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.store.show_shortcut_bar = !self.store.show_shortcut_bar;
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::Changed);
        cx.notify();
    }

    /// Set shortcut bar visibility.
    pub fn set_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.store.show_shortcut_bar != visible {
            self.store.show_shortcut_bar = visible;
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Add a shortcut and trigger save.
    pub fn add_shortcut(&mut self, entry: ShortcutEntry, cx: &mut Context<Self>) {
        let id = entry.id;
        self.store.add_shortcut(entry);
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::ShortcutAdded(id));
        cx.notify();
    }

    /// Remove shortcut and trigger save.
    pub fn remove_shortcut(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_shortcut(id) {
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::ShortcutRemoved(id));
            cx.notify();
        }
    }

    /// Update a shortcut and trigger save.
    pub fn update_shortcut(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut ShortcutEntry),
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.store.find_shortcut_mut(id) {
            update_fn(entry);
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle a shortcut's enabled state.
    pub fn toggle_shortcut(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(entry) = self.store.find_shortcut_mut(id) {
            entry.enabled = !entry.enabled;
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Set a shortcut's enabled state.
    pub fn set_shortcut_enabled(&mut self, id: Uuid, enabled: bool, cx: &mut Context<Self>) {
        if let Some(entry) = self.store.find_shortcut_mut(id) {
            if entry.enabled != enabled {
                entry.enabled = enabled;
                self.schedule_save(cx);
                cx.emit(ShortcutBarStoreEvent::Changed);
                cx.notify();
            }
        }
    }

    /// Get all shortcuts.
    pub fn shortcuts(&self) -> &[ShortcutEntry] {
        &self.store.shortcuts
    }

    /// Get only enabled shortcuts.
    pub fn enabled_shortcuts(&self) -> Vec<&ShortcutEntry> {
        self.store.enabled_shortcuts()
    }

    /// Get only disabled shortcuts.
    pub fn disabled_shortcuts(&self) -> Vec<&ShortcutEntry> {
        self.store.disabled_shortcuts()
    }

    /// Find a shortcut by ID.
    pub fn find_shortcut(&self, id: Uuid) -> Option<&ShortcutEntry> {
        self.store.find_shortcut(id)
    }

    /// Find a shortcut by action type.
    pub fn find_by_action_type(&self, action_type: &str) -> Option<&ShortcutEntry> {
        self.store.find_by_action_type(action_type)
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::shortcut_bar_file()) {
                log::error!("Failed to save shortcut bar config: {}", err);
            }
        }));
    }
}
