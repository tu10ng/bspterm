use std::fs;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::highlight_rule::{HighlightProtocol, HighlightRule, TerminalTokenType};

fn default_true() -> bool {
    true
}

/// The highlight store containing all highlight rule configurations.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HighlightStore {
    pub version: u32,
    #[serde(default)]
    pub rules: Vec<HighlightRule>,
    #[serde(default = "default_true")]
    pub highlighting_enabled: bool,
}

impl HighlightStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            rules: Vec::new(),
            highlighting_enabled: true,
        }
    }

    /// Create a new store with default rules.
    pub fn with_defaults() -> Self {
        let mut store = Self::new();
        store.rules = Self::default_rules();
        store
    }

    /// Load from file, falling back to defaults if the file doesn't exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::with_defaults());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save the store to a file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Add a highlight rule.
    pub fn add_rule(&mut self, rule: HighlightRule) {
        self.rules.push(rule);
        self.sort_rules_by_priority();
    }

    /// Remove a highlight rule by ID.
    pub fn remove_rule(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.rules.iter().position(|r| r.id == id) {
            self.rules.remove(pos);
            return true;
        }
        false
    }

    /// Find a rule by ID.
    pub fn find_rule(&self, id: Uuid) -> Option<&HighlightRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Find a mutable rule by ID.
    pub fn find_rule_mut(&mut self, id: Uuid) -> Option<&mut HighlightRule> {
        self.rules.iter_mut().find(|r| r.id == id)
    }

    /// Get rules filtered by protocol.
    pub fn rules_for_protocol(&self, protocol: Option<&HighlightProtocol>) -> Vec<&HighlightRule> {
        self.rules
            .iter()
            .filter(|r| r.enabled && r.protocol.matches(protocol))
            .collect()
    }

    /// Get enabled rules sorted by priority.
    pub fn enabled_rules(&self) -> Vec<&HighlightRule> {
        self.rules.iter().filter(|r| r.enabled).collect()
    }

    /// Move a rule to a new position.
    pub fn move_rule(&mut self, id: Uuid, new_index: usize) -> bool {
        let Some(current_index) = self.rules.iter().position(|r| r.id == id) else {
            return false;
        };

        if current_index == new_index {
            return true;
        }

        let rule = self.rules.remove(current_index);
        let insert_index = if new_index > current_index {
            new_index.saturating_sub(1).min(self.rules.len())
        } else {
            new_index.min(self.rules.len())
        };
        self.rules.insert(insert_index, rule);
        true
    }

    /// Sort rules by priority (higher priority first).
    fn sort_rules_by_priority(&mut self) {
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Get the default highlight rules.
    pub fn default_rules() -> Vec<HighlightRule> {
        vec![
            // Error keywords - highest priority
            HighlightRule::new(
                "Error",
                r"(?i)\b(error|fail(ed)?|fatal|critical|exception|panic)\b",
                TerminalTokenType::Error,
            )
            .with_case_insensitive(true)
            .with_priority(100),
            // Warning keywords
            HighlightRule::new(
                "Warning",
                r"(?i)\b(warn(ing)?|caution|deprecated)\b",
                TerminalTokenType::Warning,
            )
            .with_case_insensitive(true)
            .with_priority(90),
            // Success keywords
            HighlightRule::new(
                "Success",
                r"(?i)\b(success(ful)?|passed|ok|done|completed)\b",
                TerminalTokenType::Success,
            )
            .with_case_insensitive(true)
            .with_priority(85),
            // Info keywords
            HighlightRule::new("Info", r"(?i)\binfo\b", TerminalTokenType::Info)
                .with_case_insensitive(true)
                .with_priority(80),
            // Debug keywords
            HighlightRule::new(
                "Debug",
                r"(?i)\b(debug|trace)\b",
                TerminalTokenType::Debug,
            )
            .with_case_insensitive(true)
            .with_priority(70),
            // ISO Timestamp (2024-01-15T10:30:45 or 2024-01-15 10:30:45)
            HighlightRule::new(
                "ISO Timestamp",
                r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}",
                TerminalTokenType::Timestamp,
            )
            .with_priority(50),
            // Common log timestamp (Jan 15 10:30:45)
            HighlightRule::new(
                "Log Timestamp",
                r"(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}",
                TerminalTokenType::Timestamp,
            )
            .with_priority(50),
            // IP Address (IPv4)
            HighlightRule::new(
                "IPv4 Address",
                r"\b(?:\d{1,3}\.){3}\d{1,3}\b",
                TerminalTokenType::IpAddress,
            )
            .with_priority(60),
            // URL
            HighlightRule::new("URL", r#"https?://[^\s<>"']+"#, TerminalTokenType::Url)
                .with_priority(55),
            // File path (Unix-style)
            HighlightRule::new(
                "Unix Path",
                r"(?:/[a-zA-Z0-9._-]+)+(?:/[a-zA-Z0-9._-]*)?",
                TerminalTokenType::Path,
            )
            .with_priority(40),
        ]
    }
}

/// Events emitted by the highlight store for UI subscription.
#[derive(Clone, Debug)]
pub enum HighlightStoreEvent {
    Changed,
    RuleAdded(Uuid),
    RuleRemoved(Uuid),
    HighlightingToggled(bool),
}

/// Global marker for cx.global access.
pub struct GlobalHighlightStore(pub Entity<HighlightStoreEntity>);
impl Global for GlobalHighlightStore {}

/// GPUI Entity wrapping HighlightStore.
pub struct HighlightStoreEntity {
    store: HighlightStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<HighlightStoreEvent> for HighlightStoreEntity {}

impl HighlightStoreEntity {
    /// Initialize global highlight store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalHighlightStore>().is_some() {
            return;
        }

        let store =
            HighlightStore::load_from_file(paths::highlight_rules_file()).unwrap_or_else(|err| {
                log::error!("Failed to load highlight rules config: {}", err);
                HighlightStore::with_defaults()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalHighlightStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalHighlightStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalHighlightStore>()
            .map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &HighlightStore {
        &self.store
    }

    /// Get whether highlighting is enabled.
    pub fn highlighting_enabled(&self) -> bool {
        self.store.highlighting_enabled
    }

    /// Toggle highlighting enabled state.
    pub fn toggle_highlighting(&mut self, cx: &mut Context<Self>) {
        self.store.highlighting_enabled = !self.store.highlighting_enabled;
        self.schedule_save(cx);
        cx.emit(HighlightStoreEvent::HighlightingToggled(
            self.store.highlighting_enabled,
        ));
        cx.notify();
    }

    /// Set highlighting enabled state.
    pub fn set_highlighting_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.store.highlighting_enabled != enabled {
            self.store.highlighting_enabled = enabled;
            self.schedule_save(cx);
            cx.emit(HighlightStoreEvent::HighlightingToggled(enabled));
            cx.notify();
        }
    }

    /// Add a rule and trigger save.
    pub fn add_rule(&mut self, rule: HighlightRule, cx: &mut Context<Self>) {
        let id = rule.id;
        self.store.add_rule(rule);
        self.schedule_save(cx);
        cx.emit(HighlightStoreEvent::RuleAdded(id));
        cx.notify();
    }

    /// Remove a rule and trigger save.
    pub fn remove_rule(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_rule(id) {
            self.schedule_save(cx);
            cx.emit(HighlightStoreEvent::RuleRemoved(id));
            cx.notify();
        }
    }

    /// Update a rule and trigger save.
    pub fn update_rule(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut HighlightRule),
        cx: &mut Context<Self>,
    ) {
        if let Some(rule) = self.store.find_rule_mut(id) {
            update_fn(rule);
            self.schedule_save(cx);
            cx.emit(HighlightStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle a rule's enabled state.
    pub fn toggle_rule_enabled(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(rule) = self.store.find_rule_mut(id) {
            rule.enabled = !rule.enabled;
            self.schedule_save(cx);
            cx.emit(HighlightStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Move a rule to a new position.
    pub fn move_rule(&mut self, id: Uuid, new_index: usize, cx: &mut Context<Self>) {
        if self.store.move_rule(id, new_index) {
            self.schedule_save(cx);
            cx.emit(HighlightStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Get all rules.
    pub fn rules(&self) -> &[HighlightRule] {
        &self.store.rules
    }

    /// Get enabled rules.
    pub fn enabled_rules(&self) -> Vec<&HighlightRule> {
        self.store.enabled_rules()
    }

    /// Get rules for a specific protocol.
    pub fn rules_for_protocol(&self, protocol: Option<&HighlightProtocol>) -> Vec<&HighlightRule> {
        self.store.rules_for_protocol(protocol)
    }

    /// Find a rule by ID.
    pub fn find_rule(&self, id: Uuid) -> Option<&HighlightRule> {
        self.store.find_rule(id)
    }

    /// Reset to default rules.
    pub fn reset_to_defaults(&mut self, cx: &mut Context<Self>) {
        self.store.rules = HighlightStore::default_rules();
        self.schedule_save(cx);
        cx.emit(HighlightStoreEvent::Changed);
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::highlight_rules_file()) {
                log::error!("Failed to save highlight rules config: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_store_serialization() {
        let store = HighlightStore::with_defaults();
        let json = serde_json::to_string_pretty(&store).unwrap();
        let deserialized: HighlightStore = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, HighlightStore::CURRENT_VERSION);
        assert!(!deserialized.rules.is_empty());
        assert!(deserialized.highlighting_enabled);
    }

    #[test]
    fn test_default_rules_contain_expected() {
        let rules = HighlightStore::default_rules();

        let has_error = rules
            .iter()
            .any(|r| r.token_type == TerminalTokenType::Error);
        let has_warning = rules
            .iter()
            .any(|r| r.token_type == TerminalTokenType::Warning);
        let has_timestamp = rules
            .iter()
            .any(|r| r.token_type == TerminalTokenType::Timestamp);
        let has_ip = rules
            .iter()
            .any(|r| r.token_type == TerminalTokenType::IpAddress);

        assert!(has_error, "Should have error rule");
        assert!(has_warning, "Should have warning rule");
        assert!(has_timestamp, "Should have timestamp rule");
        assert!(has_ip, "Should have IP address rule");
    }

    #[test]
    fn test_rules_for_protocol() {
        let mut store = HighlightStore::new();

        let rule_all =
            HighlightRule::new("All", "test", TerminalTokenType::Info).with_protocol(HighlightProtocol::All);
        let rule_ssh =
            HighlightRule::new("SSH", "test", TerminalTokenType::Info).with_protocol(HighlightProtocol::Ssh);
        let rule_telnet = HighlightRule::new("Telnet", "test", TerminalTokenType::Info)
            .with_protocol(HighlightProtocol::Telnet);

        store.add_rule(rule_all);
        store.add_rule(rule_ssh);
        store.add_rule(rule_telnet);

        let ssh_rules = store.rules_for_protocol(Some(&HighlightProtocol::Ssh));
        assert_eq!(ssh_rules.len(), 2); // All + SSH

        let telnet_rules = store.rules_for_protocol(Some(&HighlightProtocol::Telnet));
        assert_eq!(telnet_rules.len(), 2); // All + Telnet

        let local_rules = store.rules_for_protocol(None);
        assert_eq!(local_rules.len(), 1); // Only All
    }
}
