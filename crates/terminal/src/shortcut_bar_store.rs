use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bspterm_actions::terminal_shortcut_bar::RunScriptShortcut;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, KeyBinding, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::abbr_store::AbbreviationProtocol;

fn default_true() -> bool {
    true
}

/// Get the Chinese label for an action type.
pub fn get_action_label(action: &str) -> &str {
    match action {
        "terminal::Copy" => "复制",
        "terminal::Paste" => "粘贴",
        "terminal::Clear" => "清屏",
        "terminal::ClearScrollback" => "清除滚动",
        "terminal::ScrollPageUp" => "上翻页",
        "terminal::ScrollPageDown" => "下翻页",
        "terminal::ScrollToTop" => "到顶部",
        "terminal::ScrollToBottom" => "到底部",
        "terminal::ScrollLineUp" => "上滚一行",
        "terminal::ScrollLineDown" => "下滚一行",
        "terminal::ToggleViMode" => "Vi模式",
        "terminal::ReconnectTerminal" => "重连",
        "terminal::DisconnectTerminal" => "断开",
        "editor::SelectAll" => "全选",
        _ => action,
    }
}

/// All system actions supported by the shortcut bar.
pub const ALL_SYSTEM_ACTIONS: &[&str] = &[
    "terminal::Copy",
    "terminal::Paste",
    "terminal::Clear",
    "terminal::ClearScrollback",
    "terminal::ScrollPageUp",
    "terminal::ScrollPageDown",
    "terminal::ScrollToTop",
    "terminal::ScrollToBottom",
    "terminal::ScrollLineUp",
    "terminal::ScrollLineDown",
    "terminal::ToggleViMode",
    "terminal::ReconnectTerminal",
    "terminal::DisconnectTerminal",
    "editor::SelectAll",
];

/// A visible system shortcut (keybinding + action combination).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VisibleShortcut {
    pub keybinding: String,
    pub action: String,
}

impl VisibleShortcut {
    pub fn new(keybinding: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            keybinding: keybinding.into(),
            action: action.into(),
        }
    }
}

/// A user-created script shortcut.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptShortcut {
    pub id: Uuid,
    pub label: String,
    pub keybinding: String,
    pub script_path: PathBuf,
    #[serde(default)]
    pub hidden: bool,
    /// Protocol filter: None means all protocols (including local)
    #[serde(default)]
    pub protocol: Option<AbbreviationProtocol>,
}

impl ScriptShortcut {
    pub fn new(
        label: impl Into<String>,
        keybinding: impl Into<String>,
        script_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            keybinding: keybinding.into(),
            script_path: script_path.into(),
            hidden: false,
            protocol: None,
        }
    }

    pub fn with_protocol(
        label: impl Into<String>,
        keybinding: impl Into<String>,
        script_path: impl Into<PathBuf>,
        protocol: Option<AbbreviationProtocol>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            keybinding: keybinding.into(),
            script_path: script_path.into(),
            hidden: false,
            protocol,
        }
    }
}

/// The shortcut bar configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShortcutBarConfig {
    pub version: u32,
    /// System shortcuts that are visible in the shortcut bar.
    #[serde(default)]
    pub visible_shortcuts: Vec<VisibleShortcut>,
    /// User-created script shortcuts.
    #[serde(default)]
    pub script_shortcuts: Vec<ScriptShortcut>,
    /// Whether to show the shortcut bar.
    #[serde(default = "default_true")]
    pub show_shortcut_bar: bool,
}

impl Default for ShortcutBarConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortcutBarConfig {
    pub const CURRENT_VERSION: u32 = 3;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            visible_shortcuts: vec![
                VisibleShortcut::new("ctrl-shift-c", "terminal::Copy"),
                VisibleShortcut::new("ctrl-shift-v", "terminal::Paste"),
            ],
            script_shortcuts: Vec::new(),
            show_shortcut_bar: true,
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;

        if let Ok(config) = serde_json::from_str::<Self>(&content) {
            if config.version >= Self::CURRENT_VERSION {
                return Ok(config);
            }
        }

        Ok(Self::new())
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Check if a system shortcut is visible.
    pub fn is_system_shortcut_visible(&self, keybinding: &str, action: &str) -> bool {
        self.visible_shortcuts
            .iter()
            .any(|v| v.keybinding == keybinding && v.action == action)
    }

    /// Check if a system shortcut is hidden (convenience method).
    pub fn is_system_shortcut_hidden(&self, keybinding: &str, action: &str) -> bool {
        !self.is_system_shortcut_visible(keybinding, action)
    }

    /// Set visibility for a system shortcut.
    pub fn set_system_shortcut_visible(&mut self, keybinding: &str, action: &str, visible: bool) {
        if visible {
            if !self.is_system_shortcut_visible(keybinding, action) {
                self.visible_shortcuts
                    .push(VisibleShortcut::new(keybinding, action));
            }
        } else {
            self.visible_shortcuts
                .retain(|v| !(v.keybinding == keybinding && v.action == action));
        }
    }

    pub fn add_script_shortcut(&mut self, shortcut: ScriptShortcut) {
        self.script_shortcuts.push(shortcut);
    }

    pub fn remove_script_shortcut(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.script_shortcuts.iter().position(|s| s.id == id) {
            self.script_shortcuts.remove(pos);
            return true;
        }
        false
    }

    pub fn find_script_shortcut(&self, id: Uuid) -> Option<&ScriptShortcut> {
        self.script_shortcuts.iter().find(|s| s.id == id)
    }

    pub fn find_script_shortcut_mut(&mut self, id: Uuid) -> Option<&mut ScriptShortcut> {
        self.script_shortcuts.iter_mut().find(|s| s.id == id)
    }

    pub fn set_script_shortcut_hidden(&mut self, id: Uuid, hidden: bool) {
        if let Some(shortcut) = self.find_script_shortcut_mut(id) {
            shortcut.hidden = hidden;
        }
    }

    pub fn visible_script_shortcuts(&self) -> Vec<&ScriptShortcut> {
        self.script_shortcuts.iter().filter(|s| !s.hidden).collect()
    }

    /// Get visible script shortcuts filtered by protocol.
    pub fn visible_script_shortcuts_for_protocol(
        &self,
        current_protocol: Option<&AbbreviationProtocol>,
    ) -> Vec<&ScriptShortcut> {
        self.script_shortcuts
            .iter()
            .filter(|s| !s.hidden && Self::protocol_matches(&s.protocol, current_protocol))
            .collect()
    }

    /// Check if a shortcut's protocol filter matches the current terminal protocol.
    fn protocol_matches(
        shortcut_protocol: &Option<AbbreviationProtocol>,
        current_protocol: Option<&AbbreviationProtocol>,
    ) -> bool {
        match shortcut_protocol {
            None => true,
            Some(AbbreviationProtocol::All) => true,
            Some(p) => current_protocol == Some(p),
        }
    }
}

/// Events emitted by the shortcut bar store for UI subscription.
#[derive(Clone, Debug)]
pub enum ShortcutBarStoreEvent {
    Changed,
    ScriptShortcutAdded(Uuid),
    ScriptShortcutRemoved(Uuid),
}

/// Global marker for cx.global access.
pub struct GlobalShortcutBarStore(pub Entity<ShortcutBarStoreEntity>);
impl Global for GlobalShortcutBarStore {}

/// GPUI Entity wrapping ShortcutBarConfig.
pub struct ShortcutBarStoreEntity {
    config: ShortcutBarConfig,
    save_task: Option<Task<()>>,
}

impl EventEmitter<ShortcutBarStoreEvent> for ShortcutBarStoreEntity {}

impl ShortcutBarStoreEntity {
    /// Initialize global shortcut bar store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalShortcutBarStore>().is_some() {
            return;
        }

        let config = ShortcutBarConfig::load_from_file(paths::shortcut_bar_file()).unwrap_or_else(
            |err| {
                log::error!("Failed to load shortcut bar config: {}", err);
                ShortcutBarConfig::new()
            },
        );

        for shortcut in &config.script_shortcuts {
            Self::register_script_keybinding(shortcut, cx);
        }

        let entity = cx.new(|_| Self {
            config,
            save_task: None,
        });

        cx.set_global(GlobalShortcutBarStore(entity));
    }

    /// Build context predicate string based on protocol filter.
    fn build_context_predicate(protocol: &Option<AbbreviationProtocol>) -> &'static str {
        match protocol {
            Some(AbbreviationProtocol::Ssh) => "Terminal && protocol == ssh",
            Some(AbbreviationProtocol::Telnet) => "Terminal && protocol == telnet",
            Some(AbbreviationProtocol::All) | None => "Terminal",
        }
    }

    /// Register a script shortcut's keybinding in the global keymap.
    fn register_script_keybinding(shortcut: &ScriptShortcut, cx: &mut App) {
        let context_predicate = Self::build_context_predicate(&shortcut.protocol);
        let action = RunScriptShortcut {
            script_id: shortcut.id.to_string(),
        };
        match KeyBinding::new(&shortcut.keybinding, action, Some(context_predicate)).keystrokes() {
            [] => {
                log::warn!(
                    "Failed to parse keybinding for script shortcut: {}",
                    shortcut.keybinding
                );
            }
            _ => {
                cx.bind_keys([KeyBinding::new(
                    &shortcut.keybinding,
                    RunScriptShortcut {
                        script_id: shortcut.id.to_string(),
                    },
                    Some(context_predicate),
                )]);
                log::info!(
                    "Registered script shortcut keybinding: {} -> {}",
                    shortcut.keybinding,
                    shortcut.label
                );
            }
        }
    }

    /// Unregister a script shortcut's keybinding from the global keymap.
    /// This is done by binding a NoAction to the same keystroke with the same context.
    fn unregister_script_keybinding(shortcut: &ScriptShortcut, cx: &mut App) {
        let context_predicate = Self::build_context_predicate(&shortcut.protocol);
        cx.bind_keys([KeyBinding::new(
            &shortcut.keybinding,
            gpui::NoAction {},
            Some(context_predicate),
        )]);
        log::info!(
            "Unregistered script shortcut keybinding: {} -> {}",
            shortcut.keybinding,
            shortcut.label
        );
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

    /// Read-only access to config.
    pub fn config(&self) -> &ShortcutBarConfig {
        &self.config
    }

    /// Get whether shortcut bar is visible.
    pub fn show_shortcut_bar(&self) -> bool {
        self.config.show_shortcut_bar
    }

    /// Toggle shortcut bar visibility.
    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.config.show_shortcut_bar = !self.config.show_shortcut_bar;
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::Changed);
        cx.notify();
    }

    /// Set shortcut bar visibility.
    pub fn set_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.config.show_shortcut_bar != visible {
            self.config.show_shortcut_bar = visible;
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Check if a system shortcut is visible.
    pub fn is_system_shortcut_visible(&self, keybinding: &str, action: &str) -> bool {
        self.config.is_system_shortcut_visible(keybinding, action)
    }

    /// Check if a system shortcut is hidden (convenience method).
    pub fn is_system_shortcut_hidden(&self, keybinding: &str, action: &str) -> bool {
        self.config.is_system_shortcut_hidden(keybinding, action)
    }

    /// Set visibility for a system shortcut.
    pub fn set_system_shortcut_visible(
        &mut self,
        keybinding: &str,
        action: &str,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        self.config.set_system_shortcut_visible(keybinding, action, visible);
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::Changed);
        cx.notify();
    }

    /// Get all script shortcuts.
    pub fn script_shortcuts(&self) -> &[ScriptShortcut] {
        &self.config.script_shortcuts
    }

    /// Get only visible script shortcuts.
    pub fn visible_script_shortcuts(&self) -> Vec<&ScriptShortcut> {
        self.config.visible_script_shortcuts()
    }

    /// Get visible script shortcuts filtered by protocol.
    pub fn visible_script_shortcuts_for_protocol(
        &self,
        protocol: Option<&AbbreviationProtocol>,
    ) -> Vec<&ScriptShortcut> {
        self.config.visible_script_shortcuts_for_protocol(protocol)
    }

    /// Add a script shortcut.
    pub fn add_script_shortcut(
        &mut self,
        label: String,
        keybinding: String,
        script_path: PathBuf,
        protocol: Option<AbbreviationProtocol>,
        cx: &mut Context<Self>,
    ) {
        let shortcut = ScriptShortcut::with_protocol(label, keybinding, script_path, protocol);
        let id = shortcut.id;
        Self::register_script_keybinding(&shortcut, cx);
        self.config.add_script_shortcut(shortcut);
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::ScriptShortcutAdded(id));
        cx.notify();
    }

    /// Remove a script shortcut.
    pub fn remove_script_shortcut(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(shortcut) = self.config.find_script_shortcut(id) {
            Self::unregister_script_keybinding(shortcut, cx);
        }
        if self.config.remove_script_shortcut(id) {
            self.schedule_save(cx);
            cx.emit(ShortcutBarStoreEvent::ScriptShortcutRemoved(id));
            cx.notify();
        }
    }

    /// Find a script shortcut by ID.
    pub fn find_script_shortcut(&self, id: Uuid) -> Option<&ScriptShortcut> {
        self.config.find_script_shortcut(id)
    }

    /// Set visibility for a script shortcut.
    pub fn set_script_shortcut_hidden(&mut self, id: Uuid, hidden: bool, cx: &mut Context<Self>) {
        self.config.set_script_shortcut_hidden(id, hidden);
        self.schedule_save(cx);
        cx.emit(ShortcutBarStoreEvent::Changed);
        cx.notify();
    }

    /// Get all visible system shortcuts.
    pub fn visible_shortcuts(&self) -> &[VisibleShortcut] {
        &self.config.visible_shortcuts
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let config = self.config.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = config.save_to_file(paths::shortcut_bar_file()) {
                log::error!("Failed to save shortcut bar config: {}", err);
            }
        }));
    }
}
