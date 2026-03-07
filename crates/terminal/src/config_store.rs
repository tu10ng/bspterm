use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trait for items stored in a JSON config store.
/// All items must have a unique UUID identifier.
pub trait ConfigItem {
    fn id(&self) -> Uuid;
}

/// Trait for JSON-file-backed configuration stores.
///
/// Provides default implementations for common operations:
/// - File persistence (load/save)
/// - CRUD operations on items
/// - Item reordering
///
/// Implementors must provide access to items and a file path.
pub trait JsonConfigStore: Sized + Clone + Serialize + for<'de> Deserialize<'de> {
    type Item: ConfigItem + Clone;

    /// Access the items collection.
    fn items(&self) -> &[Self::Item];

    /// Mutable access to the items collection.
    fn items_mut(&mut self) -> &mut Vec<Self::Item>;

    /// Create a new empty store.
    fn new_empty() -> Self;

    /// Load from a JSON file, falling back to a new store if the file doesn't exist.
    fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new_empty());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to a JSON file, creating parent directories as needed.
    fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Add an item to the store.
    fn add_item(&mut self, item: Self::Item) {
        self.items_mut().push(item);
    }

    /// Remove an item by ID. Returns true if found and removed.
    fn remove_item(&mut self, id: Uuid) -> bool {
        let items = self.items_mut();
        if let Some(position) = items.iter().position(|item| item.id() == id) {
            items.remove(position);
            return true;
        }
        false
    }

    /// Find an item by ID.
    fn find_item(&self, id: Uuid) -> Option<&Self::Item> {
        self.items().iter().find(|item| item.id() == id)
    }

    /// Find a mutable item by ID.
    fn find_item_mut(&mut self, id: Uuid) -> Option<&mut Self::Item> {
        self.items_mut().iter_mut().find(|item| item.id() == id)
    }

    /// Move an item to a new position. Returns true if successful.
    fn move_item(&mut self, id: Uuid, new_index: usize) -> bool {
        let items = self.items_mut();
        let Some(current_index) = items.iter().position(|item| item.id() == id) else {
            return false;
        };

        if current_index == new_index {
            return true;
        }

        let item = items.remove(current_index);
        let insert_index = if new_index > current_index {
            new_index.saturating_sub(1).min(items.len())
        } else {
            new_index.min(items.len())
        };
        items.insert(insert_index, item);
        true
    }
}

pub fn default_true() -> bool {
    true
}
