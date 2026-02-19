use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_enabled() -> bool {
    true
}

/// Configuration for a single button in the button bar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ButtonConfig {
    pub id: Uuid,
    pub label: String,
    pub script_path: PathBuf,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl ButtonConfig {
    pub fn new(label: impl Into<String>, script_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            script_path,
            tooltip: None,
            icon: None,
            enabled: true,
        }
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

fn default_show_button_bar() -> bool {
    true
}

/// The button bar store containing all button configurations.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ButtonBarStore {
    pub version: u32,
    #[serde(default)]
    pub buttons: Vec<ButtonConfig>,
    #[serde(default = "default_show_button_bar")]
    pub show_button_bar: bool,
}

impl ButtonBarStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            buttons: Vec::new(),
            show_button_bar: true,
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

    pub fn add_button(&mut self, button: ButtonConfig) {
        self.buttons.push(button);
    }

    pub fn remove_button(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.buttons.iter().position(|b| b.id == id) {
            self.buttons.remove(pos);
            return true;
        }
        false
    }

    pub fn find_button(&self, id: Uuid) -> Option<&ButtonConfig> {
        self.buttons.iter().find(|b| b.id == id)
    }

    pub fn find_button_mut(&mut self, id: Uuid) -> Option<&mut ButtonConfig> {
        self.buttons.iter_mut().find(|b| b.id == id)
    }

    pub fn move_button(&mut self, id: Uuid, new_index: usize) -> bool {
        let Some(current_index) = self.buttons.iter().position(|b| b.id == id) else {
            return false;
        };

        if current_index == new_index {
            return true;
        }

        let button = self.buttons.remove(current_index);
        let insert_index = if new_index > current_index {
            new_index.saturating_sub(1).min(self.buttons.len())
        } else {
            new_index.min(self.buttons.len())
        };
        self.buttons.insert(insert_index, button);
        true
    }
}

/// Events emitted by the button bar store for UI subscription.
#[derive(Clone, Debug)]
pub enum ButtonBarStoreEvent {
    Changed,
    ButtonAdded(Uuid),
    ButtonRemoved(Uuid),
}

/// Global marker for cx.global access.
pub struct GlobalButtonBarStore(pub Entity<ButtonBarStoreEntity>);
impl Global for GlobalButtonBarStore {}

/// GPUI Entity wrapping ButtonBarStore.
pub struct ButtonBarStoreEntity {
    store: ButtonBarStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<ButtonBarStoreEvent> for ButtonBarStoreEntity {}

impl ButtonBarStoreEntity {
    /// Initialize global button bar store on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalButtonBarStore>().is_some() {
            return;
        }

        let store = ButtonBarStore::load_from_file(paths::button_bar_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load button bar config: {}", err);
                ButtonBarStore::new()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalButtonBarStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalButtonBarStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalButtonBarStore>().map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &ButtonBarStore {
        &self.store
    }

    /// Get whether button bar is visible.
    pub fn show_button_bar(&self) -> bool {
        self.store.show_button_bar
    }

    /// Toggle button bar visibility.
    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.store.show_button_bar = !self.store.show_button_bar;
        self.schedule_save(cx);
        cx.emit(ButtonBarStoreEvent::Changed);
        cx.notify();
    }

    /// Set button bar visibility.
    pub fn set_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.store.show_button_bar != visible {
            self.store.show_button_bar = visible;
            self.schedule_save(cx);
            cx.emit(ButtonBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Add a button and trigger save.
    pub fn add_button(&mut self, button: ButtonConfig, cx: &mut Context<Self>) {
        let id = button.id;
        self.store.add_button(button);
        self.schedule_save(cx);
        cx.emit(ButtonBarStoreEvent::ButtonAdded(id));
        cx.notify();
    }

    /// Remove button and trigger save.
    pub fn remove_button(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_button(id) {
            self.schedule_save(cx);
            cx.emit(ButtonBarStoreEvent::ButtonRemoved(id));
            cx.notify();
        }
    }

    /// Update a button and trigger save.
    pub fn update_button(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut ButtonConfig),
        cx: &mut Context<Self>,
    ) {
        if let Some(button) = self.store.find_button_mut(id) {
            update_fn(button);
            self.schedule_save(cx);
            cx.emit(ButtonBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Move a button to a new position and trigger save.
    pub fn move_button(&mut self, id: Uuid, new_index: usize, cx: &mut Context<Self>) {
        if self.store.move_button(id, new_index) {
            self.schedule_save(cx);
            cx.emit(ButtonBarStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Get all buttons.
    pub fn buttons(&self) -> &[ButtonConfig] {
        &self.store.buttons
    }

    /// Get only enabled buttons.
    pub fn enabled_buttons(&self) -> Vec<&ButtonConfig> {
        self.store.buttons.iter().filter(|b| b.enabled).collect()
    }

    /// Find a button by ID.
    pub fn find_button(&self, id: Uuid) -> Option<&ButtonConfig> {
        self.store.find_button(id)
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::button_bar_file()) {
                log::error!("Failed to save button bar config: {}", err);
            }
        }));
    }
}
