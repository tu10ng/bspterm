use std::fs;
use std::net::IpAddr;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};

/// A local user profile containing employee information.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalUserProfile {
    pub employee_id: String,
    pub name: String,
}

impl LocalUserProfile {
    pub fn new(employee_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            employee_id: employee_id.into(),
            name: name.into(),
        }
    }

    pub fn initials(&self) -> String {
        self.name
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

/// A network interface with its IP address.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: IpAddr,
}

/// The local user store containing user profile and network info.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalUserStore {
    pub version: u32,
    pub profile: Option<LocalUserProfile>,
}

impl LocalUserStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            profile: None,
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
}

/// Events emitted by the local user store.
#[derive(Clone, Debug)]
pub enum LocalUserStoreEvent {
    ProfileChanged,
    NetworkInterfacesChanged,
}

/// Global marker for cx.global access.
pub struct GlobalLocalUserStore(pub Entity<LocalUserStoreEntity>);
impl Global for GlobalLocalUserStore {}

/// GPUI Entity wrapping LocalUserStore.
pub struct LocalUserStoreEntity {
    store: LocalUserStore,
    network_interfaces: Vec<NetworkInterface>,
    save_task: Option<Task<()>>,
}

impl EventEmitter<LocalUserStoreEvent> for LocalUserStoreEntity {}

impl LocalUserStoreEntity {
    /// Initialize global local user store on app startup.
    pub fn init(cx: &mut App) {
        let store = LocalUserStore::load_from_file(paths::local_user_file()).unwrap_or_else(|err| {
            log::error!("Failed to load local user: {}", err);
            LocalUserStore::new()
        });

        let network_interfaces = Self::detect_network_interfaces();

        let entity = cx.new(|_| Self {
            store,
            network_interfaces,
            save_task: None,
        });

        cx.set_global(GlobalLocalUserStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalLocalUserStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalLocalUserStore>().map(|g| g.0.clone())
    }

    /// Get the user profile.
    pub fn profile(&self) -> Option<&LocalUserProfile> {
        self.store.profile.as_ref()
    }

    /// Check if user is logged in (has profile).
    pub fn is_logged_in(&self) -> bool {
        self.store.profile.is_some()
    }

    /// Get network interfaces.
    pub fn network_interfaces(&self) -> &[NetworkInterface] {
        &self.network_interfaces
    }

    /// Get all IP addresses.
    pub fn ip_addresses(&self) -> Vec<IpAddr> {
        self.network_interfaces.iter().map(|i| i.ip).collect()
    }

    /// Set the user profile and save.
    pub fn set_profile(&mut self, profile: LocalUserProfile, cx: &mut Context<Self>) {
        self.store.profile = Some(profile);
        self.schedule_save(cx);
        cx.emit(LocalUserStoreEvent::ProfileChanged);
        cx.notify();
    }

    /// Clear the user profile.
    pub fn clear_profile(&mut self, cx: &mut Context<Self>) {
        self.store.profile = None;
        self.schedule_save(cx);
        cx.emit(LocalUserStoreEvent::ProfileChanged);
        cx.notify();
    }

    /// Refresh network interfaces.
    pub fn refresh_network_interfaces(&mut self, cx: &mut Context<Self>) {
        self.network_interfaces = Self::detect_network_interfaces();
        cx.emit(LocalUserStoreEvent::NetworkInterfacesChanged);
        cx.notify();
    }

    /// Detect all network interfaces with valid IP addresses.
    fn detect_network_interfaces() -> Vec<NetworkInterface> {
        let mut interfaces = Vec::new();

        match get_if_addrs::get_if_addrs() {
            Ok(addrs) => {
                for iface in addrs {
                    let ip = iface.ip();

                    if ip.is_loopback() {
                        continue;
                    }

                    if let IpAddr::V4(v4) = ip {
                        if v4.is_link_local() {
                            continue;
                        }
                    }

                    interfaces.push(NetworkInterface {
                        name: iface.name.clone(),
                        ip,
                    });
                }
            }
            Err(err) => {
                log::error!("Failed to detect network interfaces: {}", err);
            }
        }

        interfaces
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::local_user_file()) {
                log::error!("Failed to save local user: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_user_profile() {
        let profile = LocalUserProfile::new("12345", "Zhang San");
        assert_eq!(profile.employee_id, "12345");
        assert_eq!(profile.name, "Zhang San");
        assert_eq!(profile.initials(), "ZH");
    }

    #[test]
    fn test_local_user_store_serialization() {
        let mut store = LocalUserStore::new();
        store.profile = Some(LocalUserProfile::new("12345", "Li Si"));

        let json = serde_json::to_string(&store).expect("serialize");
        let restored: LocalUserStore = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.version, store.version);
        assert!(restored.profile.is_some());
        assert_eq!(restored.profile.as_ref().unwrap().employee_id, "12345");
    }

    #[test]
    fn test_detect_network_interfaces() {
        let interfaces = LocalUserStoreEntity::detect_network_interfaces();
        for iface in &interfaces {
            assert!(!iface.ip.is_loopback());
            if let IpAddr::V4(v4) = iface.ip {
                assert!(!v4.is_link_local());
            }
        }
    }
}
