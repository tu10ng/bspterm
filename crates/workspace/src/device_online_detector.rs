use collections::HashMap;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Task};
use serde::Serialize;
use settings::{DeviceOnlineAction, Settings};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use terminal::terminal_settings::TerminalSettings;
use util::desktop_notification::DesktopNotification;
use uuid::Uuid;

/// Information about an online device for notification grouping.
#[derive(Clone, Debug)]
pub struct OnlineDeviceInfo {
    pub terminal_id: gpui::EntityId,
    pub host: String,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
}

/// Serializable version for JSON export.
#[derive(Serialize)]
struct TerminalInfoJson {
    terminal_id: String,
    host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_name: Option<String>,
}

impl From<&OnlineDeviceInfo> for TerminalInfoJson {
    fn from(t: &OnlineDeviceInfo) -> Self {
        Self {
            terminal_id: format!("{}", t.terminal_id),
            host: t.host.clone(),
            group_id: t.group_id.map(|id| id.to_string()),
            group_name: t.group_name.clone(),
        }
    }
}

/// Events emitted by the device online detector.
#[derive(Clone, Debug)]
pub enum DeviceOnlineEvent {
    NotificationSent,
}

impl EventEmitter<DeviceOnlineEvent> for DeviceOnlineDetector {}

/// Central manager for grouped device online notifications.
///
/// This manager collects device online events and groups them by session group
/// (from remote explorer) before sending combined desktop notifications.
pub struct DeviceOnlineDetector {
    pending_notifications: HashMap<Option<Uuid>, Vec<OnlineDeviceInfo>>,
    debounce_task: Option<Task<()>>,
}

impl DeviceOnlineDetector {
    pub fn new() -> Self {
        Self {
            pending_notifications: HashMap::default(),
            debounce_task: None,
        }
    }

    /// Queue a notification for a device that came online.
    /// Notifications are grouped by group_id and debounced for 500ms.
    pub fn notify_device_online(
        &mut self,
        terminal_info: OnlineDeviceInfo,
        cx: &mut Context<Self>,
    ) {
        log::info!(
            "[DEVICE-ONLINE] Queuing notification for host: {}, group_id: {:?}",
            terminal_info.host,
            terminal_info.group_id
        );

        let group_id = terminal_info.group_id;
        self.pending_notifications
            .entry(group_id)
            .or_default()
            .push(terminal_info);

        // Cancel any existing debounce task
        self.debounce_task.take();

        // Start new debounce task (500ms)
        self.debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            this.update(cx, |this, cx| {
                this.flush_notifications(cx);
            })
            .ok();
        }));
    }

    /// Send all pending notifications, grouping by session group.
    fn flush_notifications(&mut self, cx: &mut Context<Self>) {
        let pending = std::mem::take(&mut self.pending_notifications);

        log::info!(
            "[DEVICE-ONLINE] Flushing {} notification groups",
            pending.len()
        );

        let settings = TerminalSettings::get_global(cx);
        let action = settings.device_online_action;
        let script_path = settings.device_online_script.clone();

        // Collect all terminals across all groups for script execution
        let all_terminals: Vec<OnlineDeviceInfo> =
            pending.values().flatten().cloned().collect();

        match action {
            DeviceOnlineAction::Notify => {
                for (group_id, terminals) in pending {
                    if terminals.is_empty() {
                        continue;
                    }

                    log::info!(
                        "[DEVICE-ONLINE] Sending notification: {} terminals, group_id={:?}",
                        terminals.len(),
                        group_id
                    );

                    let notification = self.build_notification(&terminals, group_id);

                    cx.spawn(async move |_, _| {
                        if let Err(e) = notification.send().await {
                            log::warn!("[DEVICE-ONLINE] Failed to send notification: {}", e);
                        }
                    })
                    .detach();
                }
            }
            DeviceOnlineAction::RunScript => {
                if all_terminals.is_empty() {
                    log::debug!("[DEVICE-ONLINE] No terminals to process for script");
                } else if let Some(ref script) = script_path {
                    log::info!(
                        "[DEVICE-ONLINE] Running script for {} terminals: {}",
                        all_terminals.len(),
                        script
                    );
                    self.run_device_online_script(&all_terminals, script, cx);
                } else {
                    log::warn!(
                        "[DEVICE-ONLINE] device_online_action is run_script but no script configured, falling back to notification"
                    );
                    for (group_id, terminals) in pending {
                        if terminals.is_empty() {
                            continue;
                        }
                        let notification = self.build_notification(&terminals, group_id);
                        cx.spawn(async move |_, _| {
                            notification.send().await.ok();
                        })
                        .detach();
                    }
                }
            }
        }

        cx.emit(DeviceOnlineEvent::NotificationSent);
    }

    /// Run the device online script with terminal information.
    fn run_device_online_script(
        &self,
        terminals: &[OnlineDeviceInfo],
        script_path: &str,
        cx: &mut Context<Self>,
    ) {
        let terminals_json: Vec<TerminalInfoJson> = terminals.iter().map(Into::into).collect();

        let json_str = match serde_json::to_string(&terminals_json) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "[DEVICE-ONLINE] Failed to serialize terminals JSON: {}",
                    e
                );
                return;
            }
        };

        let script_full_path = self.resolve_script_path(script_path);

        log::info!(
            "[DEVICE-ONLINE] Executing script: {:?} with {} terminals",
            script_full_path,
            terminals.len()
        );

        cx.background_spawn(async move {
            Self::execute_script(script_full_path, json_str).await;
        })
        .detach();
    }

    /// Resolve script path, handling relative paths from config directory.
    fn resolve_script_path(&self, script_path: &str) -> PathBuf {
        let path = PathBuf::from(script_path);

        if path.is_absolute() {
            path
        } else {
            paths::config_dir().join("scripts").join(script_path)
        }
    }

    /// Execute the Python script with the terminals JSON environment variable.
    #[allow(clippy::disallowed_methods)]
    async fn execute_script(script_path: PathBuf, terminals_json: String) {
        let bspterm_py_path = script_path
            .parent()
            .map(|p| p.join("bspterm.py"))
            .unwrap_or_else(|| PathBuf::from("bspterm.py"));

        let python_path = bspterm_py_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_string_lossy()
            .into_owned();

        #[cfg(not(target_os = "windows"))]
        let python_cmd = "python3";
        #[cfg(target_os = "windows")]
        let python_cmd = "python";

        let mut command = Command::new(python_cmd);
        command
            .arg(&script_path)
            .env("BSPTERM_RECONNECTED_TERMINALS", &terminals_json)
            .env("PYTHONPATH", &python_path);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        match command.output() {
            Ok(output) => {
                if output.status.success() {
                    log::info!(
                        "[DEVICE-ONLINE] Script executed successfully: {:?}",
                        script_path
                    );
                    if !output.stdout.is_empty() {
                        log::debug!(
                            "[DEVICE-ONLINE] Script stdout: {}",
                            String::from_utf8_lossy(&output.stdout)
                        );
                    }
                } else {
                    log::warn!(
                        "[DEVICE-ONLINE] Script exited with code {:?}: {:?}",
                        output.status.code(),
                        script_path
                    );
                    if !output.stderr.is_empty() {
                        log::warn!(
                            "[DEVICE-ONLINE] Script stderr: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "[DEVICE-ONLINE] Failed to execute script {:?}: {}",
                    script_path,
                    e
                );
            }
        }
    }

    /// Build a desktop notification for a group of reconnected terminals.
    fn build_notification(
        &self,
        terminals: &[OnlineDeviceInfo],
        group_id: Option<Uuid>,
    ) -> DesktopNotification {
        let title = "Terminal Reconnected";

        let body = if terminals.len() == 1 {
            // Single terminal
            format!("Connected to {}", terminals[0].host)
        } else if let Some(group_name) = group_id
            .and_then(|_| terminals.first())
            .and_then(|t| t.group_name.as_ref())
        {
            // Multiple terminals in a named group
            format!(
                "{} terminals reconnected in {}",
                terminals.len(),
                group_name
            )
        } else {
            // Multiple ungrouped terminals
            format!("{} terminals reconnected", terminals.len())
        };

        DesktopNotification::new(title, body)
    }
}

impl Default for DeviceOnlineDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Global marker for accessing the DeviceOnlineDetector via cx.global()
pub struct GlobalDeviceOnlineDetector(pub Entity<DeviceOnlineDetector>);

impl gpui::Global for GlobalDeviceOnlineDetector {}

impl GlobalDeviceOnlineDetector {
    /// Initialize the global device online detector.
    pub fn init(cx: &mut App) {
        let detector = cx.new(|_| DeviceOnlineDetector::new());
        cx.set_global(GlobalDeviceOnlineDetector(detector));
    }

    /// Get the global device online detector entity.
    pub fn global(cx: &App) -> Entity<DeviceOnlineDetector> {
        cx.global::<GlobalDeviceOnlineDetector>().0.clone()
    }

    /// Try to get the global device online detector entity if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<DeviceOnlineDetector>> {
        cx.try_global::<GlobalDeviceOnlineDetector>()
            .map(|g| g.0.clone())
    }
}
