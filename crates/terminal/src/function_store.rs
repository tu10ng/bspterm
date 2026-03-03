use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

/// Protocol type for function filtering.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionProtocol {
    #[default]
    All,
    Ssh,
    Telnet,
}

impl FunctionProtocol {
    pub fn label(&self) -> &'static str {
        match self {
            FunctionProtocol::All => "通用",
            FunctionProtocol::Ssh => "SSH",
            FunctionProtocol::Telnet => "Telnet",
        }
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
    pub protocol: FunctionProtocol,
    #[serde(default)]
    pub description: Option<String>,
}

impl FunctionConfig {
    pub fn new(name: impl Into<String>, script_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            script_path,
            enabled: true,
            protocol: FunctionProtocol::All,
            description: None,
        }
    }

    pub fn with_protocol(
        name: impl Into<String>,
        script_path: PathBuf,
        protocol: FunctionProtocol,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            script_path,
            enabled: true,
            protocol,
            description: None,
        }
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
}

impl FunctionStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            functions: Vec::new(),
            function_enabled: true,
            show_function_bar: true,
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

    pub fn add_function(&mut self, func: FunctionConfig) {
        self.functions.push(func);
    }

    pub fn remove_function(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.functions.iter().position(|f| f.id == id) {
            self.functions.remove(pos);
            return true;
        }
        false
    }

    pub fn find_function(&self, id: Uuid) -> Option<&FunctionConfig> {
        self.functions.iter().find(|f| f.id == id)
    }

    pub fn find_function_mut(&mut self, id: Uuid) -> Option<&mut FunctionConfig> {
        self.functions.iter_mut().find(|f| f.id == id)
    }

    pub fn find_by_name(
        &self,
        name: &str,
        protocol: Option<&FunctionProtocol>,
    ) -> Option<&FunctionConfig> {
        self.functions.iter().find(|f| {
            f.enabled && f.name == name && Self::protocol_matches(&f.protocol, protocol)
        })
    }

    pub fn functions_for_protocol(
        &self,
        protocol: Option<&FunctionProtocol>,
    ) -> Vec<&FunctionConfig> {
        self.functions
            .iter()
            .filter(|f| f.enabled && Self::protocol_matches(&f.protocol, protocol))
            .collect()
    }

    fn protocol_matches(
        func_protocol: &FunctionProtocol,
        current: Option<&FunctionProtocol>,
    ) -> bool {
        match (func_protocol, current) {
            (FunctionProtocol::All, _) => true,
            (FunctionProtocol::Ssh, Some(FunctionProtocol::Ssh)) => true,
            (FunctionProtocol::Telnet, Some(FunctionProtocol::Telnet)) => true,
            _ => false,
        }
    }

    pub fn move_function(&mut self, id: Uuid, new_index: usize) -> bool {
        let Some(current_index) = self.functions.iter().position(|f| f.id == id) else {
            return false;
        };

        if current_index == new_index {
            return true;
        }

        let func = self.functions.remove(current_index);
        let insert_index = if new_index > current_index {
            new_index.saturating_sub(1).min(self.functions.len())
        } else {
            new_index.min(self.functions.len())
        };
        self.functions.insert(insert_index, func);
        true
    }
}

/// Events emitted by the function store for UI subscription.
#[derive(Clone, Debug)]
pub enum FunctionStoreEvent {
    Changed,
    FunctionAdded(Uuid),
    FunctionRemoved(Uuid),
    FunctionEnabledToggled(bool),
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
        protocol: Option<&FunctionProtocol>,
    ) -> Option<&FunctionConfig> {
        self.store.find_by_name(name, protocol)
    }

    /// Get functions for a specific protocol (for function bar display).
    pub fn functions_for_protocol(
        &self,
        protocol: Option<&FunctionProtocol>,
    ) -> Vec<&FunctionConfig> {
        self.store.functions_for_protocol(protocol)
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
