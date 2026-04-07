use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config_store::{ConfigItem, JsonConfigStore, default_true};
use crate::TerminalProtocol;

/// Kind of function: script-based or simple abbreviation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionKind {
    #[default]
    Script,
    Abbreviation {
        trigger: String,
        expansion: String,
    },
}

impl FunctionKind {
    pub fn is_abbreviation(&self) -> bool {
        matches!(self, FunctionKind::Abbreviation { .. })
    }

    pub fn is_script(&self) -> bool {
        matches!(self, FunctionKind::Script)
    }
}

/// Configuration for a single function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionConfig {
    pub id: Uuid,
    pub name: String,
    pub script_path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub protocol: TerminalProtocol,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten, default)]
    pub kind: FunctionKind,
}

impl ConfigItem for FunctionConfig {
    fn id(&self) -> Uuid {
        self.id
    }
}

impl FunctionConfig {
    pub fn new(name: impl Into<String>, script_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            script_path,
            enabled: true,
            protocol: TerminalProtocol::All,
            description: None,
            kind: FunctionKind::Script,
        }
    }

    pub fn with_protocol(
        name: impl Into<String>,
        script_path: PathBuf,
        protocol: TerminalProtocol,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            script_path,
            enabled: true,
            protocol,
            description: None,
            kind: FunctionKind::Script,
        }
    }

    pub fn new_abbreviation(
        trigger: impl Into<String>,
        expansion: impl Into<String>,
        protocol: TerminalProtocol,
    ) -> Self {
        let trigger = trigger.into();
        let name = trigger.clone();
        Self {
            id: Uuid::new_v4(),
            name,
            script_path: PathBuf::new(),
            enabled: true,
            protocol,
            description: None,
            kind: FunctionKind::Abbreviation {
                trigger,
                expansion: expansion.into(),
            },
        }
    }

    pub fn is_abbreviation(&self) -> bool {
        self.kind.is_abbreviation()
    }

    pub fn is_script(&self) -> bool {
        self.kind.is_script()
    }
}

/// The function store containing all function configurations.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionStore {
    pub version: u32,
    #[serde(default)]
    pub functions: Vec<FunctionConfig>,
    #[serde(default = "default_true")]
    pub function_enabled: bool,
    #[serde(default = "default_true")]
    pub show_function_bar: bool,
    #[serde(default = "default_true")]
    pub abbreviation_enabled: bool,
}

impl JsonConfigStore for FunctionStore {
    type Item = FunctionConfig;

    fn items(&self) -> &[FunctionConfig] {
        &self.functions
    }

    fn items_mut(&mut self) -> &mut Vec<FunctionConfig> {
        &mut self.functions
    }

    fn new_empty() -> Self {
        Self::new()
    }
}

impl FunctionStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            functions: Vec::new(),
            function_enabled: true,
            show_function_bar: true,
            abbreviation_enabled: true,
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        <Self as JsonConfigStore>::load_from_file(path)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        <Self as JsonConfigStore>::save_to_file(self, path)
    }

    pub fn add_function(&mut self, func: FunctionConfig) {
        self.add_item(func);
    }

    pub fn remove_function(&mut self, id: Uuid) -> bool {
        self.remove_item(id)
    }

    pub fn find_function(&self, id: Uuid) -> Option<&FunctionConfig> {
        self.find_item(id)
    }

    pub fn find_function_mut(&mut self, id: Uuid) -> Option<&mut FunctionConfig> {
        self.find_item_mut(id)
    }

    pub fn find_by_name(
        &self,
        name: &str,
        protocol: Option<&TerminalProtocol>,
    ) -> Option<&FunctionConfig> {
        self.functions.iter().find(|f| {
            f.enabled && f.name == name && Self::protocol_matches(&f.protocol, protocol)
        })
    }

    pub fn functions_for_protocol(
        &self,
        protocol: Option<&TerminalProtocol>,
    ) -> Vec<&FunctionConfig> {
        self.functions
            .iter()
            .filter(|f| f.enabled && Self::protocol_matches(&f.protocol, protocol))
            .collect()
    }

    fn protocol_matches(
        func_protocol: &TerminalProtocol,
        current: Option<&TerminalProtocol>,
    ) -> bool {
        match (func_protocol, current) {
            (TerminalProtocol::All, _) => true,
            (TerminalProtocol::Ssh, Some(TerminalProtocol::Ssh)) => true,
            (TerminalProtocol::Telnet, Some(TerminalProtocol::Telnet)) => true,
            (TerminalProtocol::HuaweiVrp, Some(TerminalProtocol::HuaweiVrp)) => true,
            _ => false,
        }
    }

    pub fn move_function(&mut self, id: Uuid, new_index: usize) -> bool {
        self.move_item(id, new_index)
    }

    pub fn find_abbreviation_by_trigger(
        &self,
        trigger: &str,
        protocol: Option<&TerminalProtocol>,
    ) -> Option<&FunctionConfig> {
        self.functions.iter().find(|f| {
            f.enabled
                && matches!(
                    &f.kind,
                    FunctionKind::Abbreviation {
                        trigger: t,
                        ..
                    } if t == trigger
                )
                && Self::protocol_matches(&f.protocol, protocol)
        })
    }
}

/// Events emitted by the function store for UI subscription.
#[derive(Clone, Debug)]
pub enum FunctionStoreEvent {
    Changed,
    FunctionAdded(Uuid),
    FunctionRemoved(Uuid),
    FunctionEnabledToggled(bool),
    AbbreviationEnabledToggled(bool),
}

/// Global marker for cx.global access.
pub struct GlobalFunctionStore(pub Entity<FunctionStoreEntity>);
impl Global for GlobalFunctionStore {}

/// GPUI Entity wrapping FunctionStore.
pub struct FunctionStoreEntity {
    store: FunctionStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<FunctionStoreEvent> for FunctionStoreEntity {}

impl FunctionStoreEntity {
    /// Initialize global function store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalFunctionStore>().is_some() {
            return;
        }

        let store = FunctionStore::load_from_file(paths::functions_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load function config: {}", err);
                FunctionStore::new()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalFunctionStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalFunctionStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalFunctionStore>()
            .map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &FunctionStore {
        &self.store
    }

    /// Get whether function bar is visible.
    pub fn show_function_bar(&self) -> bool {
        self.store.show_function_bar
    }

    /// Get whether function invocation is enabled.
    pub fn function_enabled(&self) -> bool {
        self.store.function_enabled
    }

    /// Toggle function bar visibility.
    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.store.show_function_bar = !self.store.show_function_bar;
        self.schedule_save(cx);
        cx.emit(FunctionStoreEvent::Changed);
        cx.notify();
    }

    /// Set function bar visibility.
    pub fn set_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.store.show_function_bar != visible {
            self.store.show_function_bar = visible;
            self.schedule_save(cx);
            cx.emit(FunctionStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle function enabled state.
    pub fn toggle_function_enabled(&mut self, cx: &mut Context<Self>) {
        self.store.function_enabled = !self.store.function_enabled;
        self.schedule_save(cx);
        cx.emit(FunctionStoreEvent::FunctionEnabledToggled(
            self.store.function_enabled,
        ));
        cx.notify();
    }

    /// Set function enabled state.
    pub fn set_function_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.store.function_enabled != enabled {
            self.store.function_enabled = enabled;
            self.schedule_save(cx);
            cx.emit(FunctionStoreEvent::FunctionEnabledToggled(enabled));
            cx.notify();
        }
    }

    /// Get whether abbreviation expansion is enabled.
    pub fn abbreviation_enabled(&self) -> bool {
        self.store.abbreviation_enabled
    }

    /// Toggle abbreviation enabled state.
    pub fn toggle_abbreviation_enabled(&mut self, cx: &mut Context<Self>) {
        self.store.abbreviation_enabled = !self.store.abbreviation_enabled;
        self.schedule_save(cx);
        cx.emit(FunctionStoreEvent::AbbreviationEnabledToggled(
            self.store.abbreviation_enabled,
        ));
        cx.notify();
    }

    /// Find an abbreviation by its trigger word, optionally filtered by protocol.
    pub fn find_abbreviation_by_trigger(
        &self,
        trigger: &str,
        protocol: Option<&TerminalProtocol>,
    ) -> Option<&FunctionConfig> {
        self.store.find_abbreviation_by_trigger(trigger, protocol)
    }

    /// Add a function and trigger save.
    pub fn add_function(&mut self, func: FunctionConfig, cx: &mut Context<Self>) {
        let id = func.id;
        self.store.add_function(func);
        self.schedule_save(cx);
        cx.emit(FunctionStoreEvent::FunctionAdded(id));
        cx.notify();
    }

    /// Remove function and trigger save.
    pub fn remove_function(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_function(id) {
            self.schedule_save(cx);
            cx.emit(FunctionStoreEvent::FunctionRemoved(id));
            cx.notify();
        }
    }

    /// Update a function and trigger save.
    pub fn update_function(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut FunctionConfig),
        cx: &mut Context<Self>,
    ) {
        if let Some(func) = self.store.find_function_mut(id) {
            update_fn(func);
            self.schedule_save(cx);
            cx.emit(FunctionStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Move a function to a new position and trigger save.
    pub fn move_function(&mut self, id: Uuid, new_index: usize, cx: &mut Context<Self>) {
        if self.store.move_function(id, new_index) {
            self.schedule_save(cx);
            cx.emit(FunctionStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Get all functions.
    pub fn functions(&self) -> &[FunctionConfig] {
        &self.store.functions
    }

    /// Get only enabled functions.
    pub fn enabled_functions(&self) -> Vec<&FunctionConfig> {
        self.store
            .functions
            .iter()
            .filter(|f| f.enabled)
            .collect()
    }

    /// Find a function by ID.
    pub fn find_function(&self, id: Uuid) -> Option<&FunctionConfig> {
        self.store.find_function(id)
    }

    /// Find a function by name, optionally filtered by protocol.
    pub fn find_by_name(
        &self,
        name: &str,
        protocol: Option<&TerminalProtocol>,
    ) -> Option<&FunctionConfig> {
        self.store.find_by_name(name, protocol)
    }

    /// Get functions for a specific protocol (for function bar display).
    pub fn functions_for_protocol(
        &self,
        protocol: Option<&TerminalProtocol>,
    ) -> Vec<&FunctionConfig> {
        self.store.functions_for_protocol(protocol)
    }

    #[cfg(test)]
    pub fn new_for_test(store: FunctionStore) -> Self {
        Self {
            store,
            save_task: None,
        }
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::functions_file()) {
                log::error!("Failed to save function config: {}", err);
            }
        }));
    }
}
