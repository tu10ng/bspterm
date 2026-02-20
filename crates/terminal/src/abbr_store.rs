use std::fs;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

/// Protocol type for abbreviation filtering.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbbreviationProtocol {
    #[default]
    All,
    Ssh,
    Telnet,
}

impl AbbreviationProtocol {
    pub fn label(&self) -> &'static str {
        match self {
            AbbreviationProtocol::All => "通用",
            AbbreviationProtocol::Ssh => "SSH",
            AbbreviationProtocol::Telnet => "Telnet",
        }
    }
}

/// Configuration for a single abbreviation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Abbreviation {
    pub id: Uuid,
    pub trigger: String,
    pub expansion: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub protocol: AbbreviationProtocol,
}

impl Abbreviation {
    pub fn new(trigger: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger: trigger.into(),
            expansion: expansion.into(),
            enabled: true,
            protocol: AbbreviationProtocol::All,
        }
    }

    pub fn with_protocol(
        trigger: impl Into<String>,
        expansion: impl Into<String>,
        protocol: AbbreviationProtocol,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger: trigger.into(),
            expansion: expansion.into(),
            enabled: true,
            protocol,
        }
    }
}

/// The abbreviation store containing all abbreviation configurations.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AbbreviationStore {
    pub version: u32,
    #[serde(default)]
    pub abbreviations: Vec<Abbreviation>,
    #[serde(default = "default_true")]
    pub expansion_enabled: bool,
    #[serde(default = "default_true")]
    pub show_abbr_bar: bool,
}

impl AbbreviationStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            abbreviations: Vec::new(),
            expansion_enabled: true,
            show_abbr_bar: true,
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

    pub fn add_abbreviation(&mut self, abbr: Abbreviation) {
        self.abbreviations.push(abbr);
    }

    pub fn remove_abbreviation(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.abbreviations.iter().position(|a| a.id == id) {
            self.abbreviations.remove(pos);
            return true;
        }
        false
    }

    pub fn find_abbreviation(&self, id: Uuid) -> Option<&Abbreviation> {
        self.abbreviations.iter().find(|a| a.id == id)
    }

    pub fn find_abbreviation_mut(&mut self, id: Uuid) -> Option<&mut Abbreviation> {
        self.abbreviations.iter_mut().find(|a| a.id == id)
    }

    pub fn find_by_trigger(
        &self,
        trigger: &str,
        protocol: Option<&AbbreviationProtocol>,
    ) -> Option<&Abbreviation> {
        self.abbreviations.iter().find(|a| {
            a.enabled && a.trigger == trigger && Self::protocol_matches(&a.protocol, protocol)
        })
    }

    pub fn abbreviations_for_protocol(
        &self,
        protocol: Option<&AbbreviationProtocol>,
    ) -> Vec<&Abbreviation> {
        self.abbreviations
            .iter()
            .filter(|a| a.enabled && Self::protocol_matches(&a.protocol, protocol))
            .collect()
    }

    fn protocol_matches(
        abbr_protocol: &AbbreviationProtocol,
        current: Option<&AbbreviationProtocol>,
    ) -> bool {
        match (abbr_protocol, current) {
            (AbbreviationProtocol::All, _) => true,
            (AbbreviationProtocol::Ssh, Some(AbbreviationProtocol::Ssh)) => true,
            (AbbreviationProtocol::Telnet, Some(AbbreviationProtocol::Telnet)) => true,
            _ => false,
        }
    }

    pub fn move_abbreviation(&mut self, id: Uuid, new_index: usize) -> bool {
        let Some(current_index) = self.abbreviations.iter().position(|a| a.id == id) else {
            return false;
        };

        if current_index == new_index {
            return true;
        }

        let abbr = self.abbreviations.remove(current_index);
        let insert_index = if new_index > current_index {
            new_index.saturating_sub(1).min(self.abbreviations.len())
        } else {
            new_index.min(self.abbreviations.len())
        };
        self.abbreviations.insert(insert_index, abbr);
        true
    }
}

/// Events emitted by the abbreviation store for UI subscription.
#[derive(Clone, Debug)]
pub enum AbbreviationStoreEvent {
    Changed,
    AbbreviationAdded(Uuid),
    AbbreviationRemoved(Uuid),
    ExpansionToggled(bool),
}

/// Global marker for cx.global access.
pub struct GlobalAbbreviationStore(pub Entity<AbbreviationStoreEntity>);
impl Global for GlobalAbbreviationStore {}

/// GPUI Entity wrapping AbbreviationStore.
pub struct AbbreviationStoreEntity {
    store: AbbreviationStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<AbbreviationStoreEvent> for AbbreviationStoreEntity {}

impl AbbreviationStoreEntity {
    /// Initialize global abbreviation store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalAbbreviationStore>().is_some() {
            return;
        }

        let store = AbbreviationStore::load_from_file(paths::abbreviations_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load abbreviation config: {}", err);
                AbbreviationStore::new()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalAbbreviationStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAbbreviationStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalAbbreviationStore>()
            .map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &AbbreviationStore {
        &self.store
    }

    /// Get whether abbreviation bar is visible.
    pub fn show_abbr_bar(&self) -> bool {
        self.store.show_abbr_bar
    }

    /// Get whether expansion is enabled.
    pub fn expansion_enabled(&self) -> bool {
        self.store.expansion_enabled
    }

    /// Toggle abbreviation bar visibility.
    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.store.show_abbr_bar = !self.store.show_abbr_bar;
        self.schedule_save(cx);
        cx.emit(AbbreviationStoreEvent::Changed);
        cx.notify();
    }

    /// Set abbreviation bar visibility.
    pub fn set_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.store.show_abbr_bar != visible {
            self.store.show_abbr_bar = visible;
            self.schedule_save(cx);
            cx.emit(AbbreviationStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle expansion enabled state.
    pub fn toggle_expansion(&mut self, cx: &mut Context<Self>) {
        self.store.expansion_enabled = !self.store.expansion_enabled;
        self.schedule_save(cx);
        cx.emit(AbbreviationStoreEvent::ExpansionToggled(
            self.store.expansion_enabled,
        ));
        cx.notify();
    }

    /// Set expansion enabled state.
    pub fn set_expansion_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.store.expansion_enabled != enabled {
            self.store.expansion_enabled = enabled;
            self.schedule_save(cx);
            cx.emit(AbbreviationStoreEvent::ExpansionToggled(enabled));
            cx.notify();
        }
    }

    /// Add an abbreviation and trigger save.
    pub fn add_abbreviation(&mut self, abbr: Abbreviation, cx: &mut Context<Self>) {
        let id = abbr.id;
        self.store.add_abbreviation(abbr);
        self.schedule_save(cx);
        cx.emit(AbbreviationStoreEvent::AbbreviationAdded(id));
        cx.notify();
    }

    /// Remove abbreviation and trigger save.
    pub fn remove_abbreviation(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_abbreviation(id) {
            self.schedule_save(cx);
            cx.emit(AbbreviationStoreEvent::AbbreviationRemoved(id));
            cx.notify();
        }
    }

    /// Update an abbreviation and trigger save.
    pub fn update_abbreviation(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut Abbreviation),
        cx: &mut Context<Self>,
    ) {
        if let Some(abbr) = self.store.find_abbreviation_mut(id) {
            update_fn(abbr);
            self.schedule_save(cx);
            cx.emit(AbbreviationStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Move an abbreviation to a new position and trigger save.
    pub fn move_abbreviation(&mut self, id: Uuid, new_index: usize, cx: &mut Context<Self>) {
        if self.store.move_abbreviation(id, new_index) {
            self.schedule_save(cx);
            cx.emit(AbbreviationStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Get all abbreviations.
    pub fn abbreviations(&self) -> &[Abbreviation] {
        &self.store.abbreviations
    }

    /// Get only enabled abbreviations.
    pub fn enabled_abbreviations(&self) -> Vec<&Abbreviation> {
        self.store
            .abbreviations
            .iter()
            .filter(|a| a.enabled)
            .collect()
    }

    /// Find an abbreviation by ID.
    pub fn find_abbreviation(&self, id: Uuid) -> Option<&Abbreviation> {
        self.store.find_abbreviation(id)
    }

    /// Find an abbreviation by trigger, optionally filtered by protocol.
    pub fn find_by_trigger(
        &self,
        trigger: &str,
        protocol: Option<&AbbreviationProtocol>,
    ) -> Option<&Abbreviation> {
        self.store.find_by_trigger(trigger, protocol)
    }

    /// Get abbreviations for a specific protocol (for abbr bar display).
    pub fn abbreviations_for_protocol(
        &self,
        protocol: Option<&AbbreviationProtocol>,
    ) -> Vec<&Abbreviation> {
        self.store.abbreviations_for_protocol(protocol)
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::abbreviations_file()) {
                log::error!("Failed to save abbreviation config: {}", err);
            }
        }));
    }
}
