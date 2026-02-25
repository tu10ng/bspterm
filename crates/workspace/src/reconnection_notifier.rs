use collections::HashMap;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Task};
use std::time::Duration;
use util::desktop_notification::DesktopNotification;
use uuid::Uuid;

/// Information about a reconnected terminal for notification grouping.
#[derive(Clone, Debug)]
pub struct ReconnectedTerminal {
    pub terminal_id: gpui::EntityId,
    pub host: String,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
}

/// Events emitted by the reconnection notifier.
#[derive(Clone, Debug)]
pub enum ReconnectionNotifierEvent {
    NotificationSent,
}

impl EventEmitter<ReconnectionNotifierEvent> for ReconnectionNotifier {}

/// Central manager for grouped reconnection notifications.
///
/// This manager collects reconnection events and groups them by session group
/// (from remote explorer) before sending combined desktop notifications.
pub struct ReconnectionNotifier {
    pending_notifications: HashMap<Option<Uuid>, Vec<ReconnectedTerminal>>,
    debounce_task: Option<Task<()>>,
}

impl ReconnectionNotifier {
    pub fn new() -> Self {
        Self {
            pending_notifications: HashMap::default(),
            debounce_task: None,
        }
    }

    /// Queue a notification for a reconnected terminal.
    /// Notifications are grouped by group_id and debounced for 500ms.
    pub fn notify_reconnected(
        &mut self,
        terminal_info: ReconnectedTerminal,
        cx: &mut Context<Self>,
    ) {
        log::info!(
            "[RECONNECT-NOTIFIER] Queuing notification for host: {}, group_id: {:?}",
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
            "[RECONNECT-NOTIFIER] Flushing {} notification groups",
            pending.len()
        );

        for (group_id, terminals) in pending {
            if terminals.is_empty() {
                continue;
            }

            log::info!(
                "[RECONNECT-NOTIFIER] Sending notification: {} terminals, group_id={:?}",
                terminals.len(),
                group_id
            );

            let notification = self.build_notification(&terminals, group_id);

            cx.spawn(async move |_, _| {
                if let Err(e) = notification.send().await {
                    log::warn!("[RECONNECT-NOTIFIER] Failed to send notification: {}", e);
                }
            })
            .detach();
        }

        cx.emit(ReconnectionNotifierEvent::NotificationSent);
    }

    /// Build a desktop notification for a group of reconnected terminals.
    fn build_notification(
        &self,
        terminals: &[ReconnectedTerminal],
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

impl Default for ReconnectionNotifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Global marker for accessing the ReconnectionNotifier via cx.global()
pub struct GlobalReconnectionNotifier(pub Entity<ReconnectionNotifier>);

impl gpui::Global for GlobalReconnectionNotifier {}

impl GlobalReconnectionNotifier {
    /// Initialize the global reconnection notifier.
    pub fn init(cx: &mut App) {
        let notifier = cx.new(|_| ReconnectionNotifier::new());
        cx.set_global(GlobalReconnectionNotifier(notifier));
    }

    /// Get the global reconnection notifier entity.
    pub fn global(cx: &App) -> Entity<ReconnectionNotifier> {
        cx.global::<GlobalReconnectionNotifier>().0.clone()
    }

    /// Try to get the global reconnection notifier entity if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<ReconnectionNotifier>> {
        cx.try_global::<GlobalReconnectionNotifier>()
            .map(|g| g.0.clone())
    }
}
