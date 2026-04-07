use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::Result;
use bspterm_actions::sftp_panel::{
    Cancel, Chmod, Confirm, Copy, CopyPath, Cut, Delete, Download, EditPath, GoUp, NewDirectory,
    NewFile, Open, Paste, RefreshDirectory, Rename, SelectNext, SelectPrevious, ToggleFilter,
    ToggleFocus, ToggleShowHiddenFiles, ToggleSortMode, ToggleWatch,
};
use editor::{Editor, EditorEvent};
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, ClickEvent, ClipboardItem,
    Context, DismissEvent, DragMoveEvent, Entity, EventEmitter, ExternalPaths, FocusHandle,
    Focusable, IntoElement, ListSizingBehavior, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Point, PromptLevel, Render, SharedString, Styled, Subscription, Task,
    UniformListScrollHandle, WeakEntity, Window, anchored, deferred, px, uniform_list,
};
use i18n::t;
use panel::PanelHeader;
use regex::Regex;
use terminal::connection::ssh::{RemoteEntry, SftpClient, SshAuthConfig, SshConfig, SshHostKey};
use terminal::sftp_store::{SftpStore, SftpStoreEntity, SftpStoreEvent};
use terminal::ConnectionInfo;
use ui::{
    prelude::*, Color, ContextMenu, Icon, IconButton, IconName, IconPosition, IconSize, Label,
    LabelSize, ListItem, ListItemSpacing, PopoverMenu, Tooltip, h_flex, v_flex,
};
use workspace::{
    Event as WorkspaceEvent, Toast, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::NotificationId,
};

const SFTP_PANEL_KEY: &str = "SftpPanel";
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SortMode {
    Name,
    ModifiedTime,
}

#[derive(Clone, Debug)]
struct FileEntry {
    entry: RemoteEntry,
    depth: usize,
    parent_path: String,
}

/// Drop target indicator for drag-and-drop.
#[derive(Clone, PartialEq)]
enum SftpDragTarget {
    IntoDir { path: String },
    BeforeEntry { path: String },
    AfterEntry { path: String },
}

/// Data attached to internal drag operations.
#[derive(Clone)]
struct DraggedSftpEntry {
    path: String,
    name: String,
    is_dir: bool,
}

struct DraggedSftpEntryView {
    name: String,
    is_dir: bool,
}

impl Render for DraggedSftpEntryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = if self.is_dir {
            IconName::Folder
        } else {
            IconName::File
        };
        h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_md()
            .shadow_md()
            .child(Icon::new(icon).color(Color::Muted).size(IconSize::Small))
            .child(Label::new(self.name.clone()))
    }
}

struct EditState {
    index: Option<usize>, // None = new entry
    is_dir: bool,
    editor: Entity<Editor>,
    _subscription: Subscription,
}

#[derive(Clone, Debug)]
struct ClipboardState {
    paths: Vec<String>,
    is_cut: bool,
}

struct SftpFileMapping {
    remote_path: String,
    client: Arc<SftpClient>,
    original_mtime: Option<u64>,
    original_md5: [u8; 16],
}

struct ConnectionState {
    status: ConnectionStatus,
    client: Option<Arc<SftpClient>>,
    current_path: String,
    home_path: Option<String>,
    entries: Vec<FileEntry>,
    expanded_dirs: HashSet<String>,
    selected_index: Option<usize>,
    watch_enabled: bool,
    watch_task: Option<Task<()>>,
    load_task: Option<Task<()>>,
    md5_cache: HashMap<String, String>,
    dir_count_cache: HashMap<String, usize>,
    md5_available: Option<bool>,
    md5_task: Option<Task<()>>,
    dir_count_task: Option<Task<()>>,
    connect_task: Option<Task<()>>,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            status: ConnectionStatus::Disconnected,
            client: None,
            current_path: "/".to_string(),
            home_path: None,
            entries: Vec::new(),
            expanded_dirs: HashSet::new(),
            selected_index: None,
            watch_enabled: true,
            watch_task: None,
            load_task: None,
            md5_cache: HashMap::new(),
            dir_count_cache: HashMap::new(),
            md5_available: None,
            md5_task: None,
            dir_count_task: None,
            connect_task: None,
        }
    }

    fn clear_metadata_caches(&mut self) {
        self.md5_cache.clear();
        self.dir_count_cache.clear();
        self.md5_available = None;
        self.md5_task = None;
        self.dir_count_task = None;
    }

    fn stop_watch(&mut self) {
        self.watch_task = None;
    }
}

pub struct SftpPanel {
    workspace: WeakEntity<Workspace>,
    sftp_store: Entity<SftpStore>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    width: Option<Pixels>,
    connections: HashMap<SshHostKey, ConnectionState>,
    active_host_key: Option<SshHostKey>,
    sort_mode: SortMode,
    show_hidden_files: bool,
    edit_state: Option<EditState>,
    clipboard: Option<ClipboardState>,
    marked_entries: HashSet<usize>,
    filter_text: String,
    filter_editor: Option<Entity<Editor>>,
    path_editor: Option<Entity<Editor>>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    _subscriptions: Vec<Subscription>,
    sftp_file_mappings: HashMap<PathBuf, SftpFileMapping>,
    followed_terminal_id: Option<gpui::EntityId>,
    drag_target: Option<SftpDragTarget>,
    hover_expand_task: Option<Task<()>>,
    cached_visible_indices: Vec<usize>,
}

impl SftpPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sftp_store = SftpStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let store_subscription =
            cx.subscribe(&sftp_store, |this, _, event, cx| match event {
                SftpStoreEvent::ClientConnected(host_key) => {
                    if let Some(conn) = this.connections.get_mut(host_key) {
                        conn.status = ConnectionStatus::Connected;
                        conn.client =
                            this.sftp_store.read(cx).get_client(host_key);
                        cx.notify();
                    }
                }
                SftpStoreEvent::ClientDisconnected(host_key) => {
                    if let Some(conn) = this.connections.get_mut(host_key) {
                        conn.status = ConnectionStatus::Disconnected;
                        conn.client = None;
                        conn.entries.clear();
                        conn.clear_metadata_caches();
                        cx.notify();
                    }
                }
            });

        let mut subscriptions = vec![store_subscription];
        if let Some(ws) = workspace.weak_handle().upgrade() {
            subscriptions.push(cx.subscribe_in(&ws, window, Self::handle_workspace_event));
        }

        Self {
            workspace: workspace.weak_handle(),
            sftp_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            width: None,
            connections: HashMap::new(),
            active_host_key: None,
            sort_mode: SortMode::Name,
            show_hidden_files: true,
            edit_state: None,
            clipboard: None,
            marked_entries: HashSet::new(),
            filter_text: String::new(),
            filter_editor: None,
            path_editor: None,
            context_menu: None,
            _subscriptions: subscriptions,
            sftp_file_mappings: HashMap::new(),
            followed_terminal_id: None,
            drag_target: None,
            hover_expand_task: None,
            cached_visible_indices: Vec::new(),
        }
    }

    fn active_connection(&self) -> Option<&ConnectionState> {
        self.active_host_key.as_ref().and_then(|k| self.connections.get(k))
    }

    fn active_connection_mut(&mut self) -> Option<&mut ConnectionState> {
        self.active_host_key.as_ref().and_then(|k| self.connections.get_mut(k))
    }

    pub fn connect(&mut self, config: SshConfig, cx: &mut Context<Self>) {
        let host_key = SshHostKey::from(&config);
        self.active_host_key = Some(host_key.clone());

        let conn = self.connections.entry(host_key.clone()).or_insert_with(ConnectionState::new);
        conn.status = ConnectionStatus::Connecting;
        conn.entries.clear();
        conn.expanded_dirs.clear();
        conn.current_path = "/".to_string();
        conn.clear_metadata_caches();
        cx.notify();

        let task = self
            .sftp_store
            .update(cx, |store, cx| store.get_or_connect(config, cx));

        let tokio_handle = gpui_tokio::Tokio::handle(cx);
        let host_key_for_task = host_key.clone();
        let connect_task = cx.spawn(async move |this, cx| {
            match task.await {
                Ok(client) => {
                    // Resolve home directory before loading
                    let client_for_realpath = client.clone();
                    let home = tokio_handle
                        .spawn(async move { client_for_realpath.realpath(".").await })
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|r| r.map_err(anyhow::Error::from));
                    this.update(cx, |this, cx| {
                        if let Some(conn) = this.connections.get_mut(&host_key_for_task) {
                            if let Ok(home) = &home {
                                conn.home_path = Some(home.clone());
                                conn.current_path = home.clone();
                            }
                            conn.connect_task = None;
                        }
                        // Only sync if this is still the active connection
                        if this.active_host_key.as_ref() == Some(&host_key_for_task) {
                            this.sync_from_terminal(cx);
                        }
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, cx| {
                        if let Some(conn) = this.connections.get_mut(&host_key_for_task) {
                            conn.status = ConnectionStatus::Error(err.to_string());
                            conn.connect_task = None;
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        });
        if let Some(conn) = self.connections.get_mut(&host_key) {
            conn.connect_task = Some(connect_task);
        }
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        let Some(host_key) = self.active_host_key.clone() else {
            return;
        };
        if let Some(conn) = self.connections.remove(&host_key) {
            drop(conn); // drop tasks
            self.sftp_store.update(cx, |store, cx| {
                store.disconnect(&host_key, cx);
            });
        }
        self.active_host_key = None;
        cx.notify();
    }

    pub fn connect_from_active_terminal(&mut self, cx: &mut Context<Self>) {
        // Check if active connection is already connecting or connected
        let active_status = self.active_connection().map(|c| &c.status);
        if matches!(active_status, Some(ConnectionStatus::Connecting | ConnectionStatus::Connected)) {
            return;
        }

        if let Some(config) = self.active_terminal_ssh_config(cx) {
            self.connect(config, cx);
        }
    }

    fn active_terminal_ssh_config(&self, cx: &mut Context<Self>) -> Option<SshConfig> {
        let workspace = self.workspace.upgrade()?;

        let connection_info = workspace.update(cx, |workspace, cx| {
            let pane = workspace.active_pane();
            let pane = pane.read(cx);
            pane.active_item().and_then(|item| {
                let terminal_view = item.downcast::<terminal_view::TerminalView>()?;
                terminal_view.read(cx).terminal().read(cx).connection_info().cloned()
            })
        });

        if let Some(ConnectionInfo::Ssh {
            host,
            port,
            username,
            password,
            private_key_path,
            passphrase,
            ..
        }) = connection_info
        {
            let auth = if let Some(key_path) = private_key_path {
                SshAuthConfig::PrivateKey {
                    path: key_path,
                    passphrase,
                }
            } else if let Some(pw) = password {
                SshAuthConfig::Password(pw)
            } else {
                SshAuthConfig::Auto
            };

            let mut config = SshConfig::new(host, port);
            if let Some(user) = username {
                config = config.with_username(user);
            }
            config = config.with_auth(auth);
            Some(config)
        } else {
            None
        }
    }

    fn handle_workspace_event(
        &mut self,
        _workspace: &Entity<Workspace>,
        event: &WorkspaceEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WorkspaceEvent::ActiveItemChanged = event {
            self.follow_active_terminal(cx);
        }
    }

    fn follow_active_terminal(&mut self, cx: &mut Context<Self>) {
        // Get the active terminal's entity id to detect actual tab switches
        let terminal_id = self.workspace.upgrade().and_then(|ws| {
            ws.read(cx).active_pane().read(cx).active_item().and_then(|item| {
                let terminal_view = item.downcast::<terminal_view::TerminalView>()?;
                Some(terminal_view.read(cx).terminal().entity_id())
            })
        });

        if terminal_id == self.followed_terminal_id {
            return;
        }
        self.followed_terminal_id = terminal_id;

        let Some(config) = self.active_terminal_ssh_config(cx) else {
            return;
        };

        let new_host_key = SshHostKey::from(&config);

        if self.active_host_key.as_ref() == Some(&new_host_key) {
            // Same host, just sync CWD
            self.sync_from_terminal(cx);
        } else if self.connections.contains_key(&new_host_key) {
            // Existing connection — switch display
            self.active_host_key = Some(new_host_key);
            self.sync_from_terminal(cx);
            cx.notify();
        } else {
            // New connection
            self.connect(config, cx);
        }
    }

    fn sync_from_terminal(&mut self, cx: &mut Context<Self>) {
        let status = self.active_connection().map(|c| c.status.clone());
        if status != Some(ConnectionStatus::Connected) {
            return;
        }

        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let prompt = workspace.update(cx, |workspace, cx| {
            let pane = workspace.active_pane();
            let pane = pane.read(cx);
            pane.active_item().and_then(|item| {
                let terminal_view = item.downcast::<terminal_view::TerminalView>()?;
                terminal_view.read(cx).terminal().read(cx).read_prompt_from_grid()
            })
        });

        if let Some(path) = prompt.as_deref().and_then(extract_cwd_from_prompt) {
            if path.starts_with('/') {
                if let Some(conn) = self.active_connection_mut() {
                    conn.current_path = path;
                    conn.entries.clear();
                    conn.selected_index = None;
                }
                self.load_directory(cx);
                cx.notify();
            } else if path.starts_with('~') {
                let home = self.active_connection().and_then(|c| c.home_path.clone());
                if let Some(home) = home {
                    let resolved = if path == "~" {
                        home.clone()
                    } else {
                        format!("{}{}", home, &path[1..])
                    };
                    if let Some(conn) = self.active_connection_mut() {
                        conn.current_path = resolved;
                        conn.entries.clear();
                        conn.selected_index = None;
                    }
                    self.load_directory(cx);
                    cx.notify();
                } else {
                    self.load_directory(cx);
                }
            } else {
                self.resolve_bare_dirname(path, cx);
            }
        } else {
            self.load_directory(cx);
        }
    }

    fn resolve_bare_dirname(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_default();
        let home_path = self.active_connection().and_then(|c| c.home_path.clone());
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn(async move |this, cx| {
            let mut candidates = Vec::new();
            candidates.push(format!("/{}", name));
            let under_current = if current_path.ends_with('/') {
                format!("{}{}", current_path, name)
            } else {
                format!("{}/{}", current_path, name)
            };
            candidates.push(under_current);
            if let Some(home) = &home_path {
                let under_home = if home.ends_with('/') {
                    format!("{}{}", home, name)
                } else {
                    format!("{}/{}", home, name)
                };
                candidates.push(under_home);
            }

            let mut seen = std::collections::HashSet::new();
            candidates.retain(|c| seen.insert(c.clone()));

            for candidate in &candidates {
                let client_clone = client.clone();
                let path = candidate.clone();
                let result = tokio_handle
                    .spawn(async move { client_clone.stat(&path).await })
                    .await;
                if let Ok(Ok(entry)) = result {
                    if entry.is_dir {
                        this.update(cx, |this, cx| {
                            if let Some(conn) = this.active_connection_mut() {
                                conn.current_path = candidate.clone();
                                conn.entries.clear();
                                conn.expanded_dirs.clear();
                                conn.selected_index = None;
                            }
                            this.load_directory(cx);
                            cx.notify();
                        }).ok();
                        return;
                    }
                }
            }

            this.update(cx, |this, cx| {
                this.load_directory(cx);
            }).ok();
        }).detach();
    }

    fn load_directory(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let Some(path) = self.active_connection().map(|c| c.current_path.clone()) else {
            return;
        };
        let active_key = self.active_host_key.clone();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let load_task = cx.spawn(async move |this, cx| {
            let result = tokio_handle
                .spawn(async move { client.list_dir(&path).await })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r);

            match result {
                Ok(entries) => {
                    this.update(cx, |this, cx| {
                        let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) else {
                            return;
                        };
                        let parent_path = conn.current_path.clone();
                        let expanded_dirs = conn.expanded_dirs.clone();
                        conn.md5_cache.clear();
                        conn.dir_count_cache.clear();
                        conn.entries = entries
                            .into_iter()
                            .map(|entry| FileEntry { depth: 0, parent_path: parent_path.clone(), entry })
                            .collect();
                        conn.load_task = None;
                        // sort_entries and re_expand_dirs need &mut self, so drop conn borrow
                        drop(expanded_dirs.clone());
                        let expanded = expanded_dirs;
                        // We need to sort and re-expand via self methods
                        // But those methods access self.entries etc. which are now in conn
                        // So we do it inline
                        let sort_mode = this.sort_mode;
                        if let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) {
                            Self::sort_entries_on(sort_mode, &mut conn.entries);
                            // Re-expand: just set expanded_dirs, children will be loaded below
                        }
                        // Re-expand directories after reload
                        if !expanded.is_empty() {
                            this.re_expand_dirs_for_key(active_key.as_ref(), expanded, cx);
                        }
                        this.fetch_entry_metadata(cx);
                        cx.notify();

                        // Start watch after successful load
                        let watch_enabled = active_key.as_ref()
                            .and_then(|k| this.connections.get(k))
                            .map(|c| c.watch_enabled)
                            .unwrap_or(false);
                        if watch_enabled {
                            this.start_watch(cx);
                        }
                    })
                    .ok();
                }
                Err(err) => {
                    log::error!("Failed to list directory: {}", err);
                    this.update(cx, |this, cx| {
                        if let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) {
                            conn.load_task = None;
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        });
        if let Some(conn) = self.active_connection_mut() {
            conn.load_task = Some(load_task);
        }
    }

    fn sort_entries_on(sort_mode: SortMode, entries: &mut Vec<FileEntry>) {
        if entries.iter().all(|e| e.depth == 0) {
            entries.sort_by(|a, b| {
                b.entry.is_dir.cmp(&a.entry.is_dir).then_with(|| match sort_mode {
                    SortMode::Name => {
                        a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase())
                    }
                    SortMode::ModifiedTime => b.entry.modified.cmp(&a.entry.modified),
                })
            });
        } else {
            Self::sort_entries_tree_on(sort_mode, entries);
        }
    }

    fn sort_entries(&mut self) {
        let sort_mode = self.sort_mode;
        if let Some(conn) = self.active_connection_mut() {
            Self::sort_entries_on(sort_mode, &mut conn.entries);
        }
    }

    fn sort_entries_tree_on(sort_mode: SortMode, entries: &mut Vec<FileEntry>) {
        let mut index = 0;
        while index < entries.len() {
            let depth = entries[index].depth;
            let parent_path = entries[index].parent_path.clone();
            let sibling_start = index;

            let mut sibling_ranges: Vec<(usize, usize)> = Vec::new();
            while index < entries.len()
                && entries[index].depth == depth
                && entries[index].parent_path == parent_path
            {
                let start = index;
                index += 1;
                while index < entries.len() && entries[index].depth > depth {
                    index += 1;
                }
                sibling_ranges.push((start, index));
            }

            if sibling_ranges.len() > 1 {
                let mut subtrees: Vec<Vec<FileEntry>> = sibling_ranges
                    .iter()
                    .map(|(start, end)| entries[*start..*end].to_vec())
                    .collect();

                subtrees.sort_by(|a, b| {
                    let a_entry = &a[0];
                    let b_entry = &b[0];
                    b_entry.entry.is_dir.cmp(&a_entry.entry.is_dir).then_with(|| match sort_mode {
                        SortMode::Name => a_entry
                            .entry
                            .name
                            .to_lowercase()
                            .cmp(&b_entry.entry.name.to_lowercase()),
                        SortMode::ModifiedTime => {
                            b_entry.entry.modified.cmp(&a_entry.entry.modified)
                        }
                    })
                });

                let mut write_index = sibling_start;
                for subtree in subtrees {
                    for entry in subtree {
                        entries[write_index] = entry;
                        write_index += 1;
                    }
                }
            }
        }
    }

    fn toggle_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.active_connection().and_then(|c| c.entries.get(index).cloned()) else {
            return;
        };
        if !entry.entry.is_dir {
            return;
        }

        let path = entry.entry.path.clone();

        let is_expanded = self.active_connection().map(|c| c.expanded_dirs.contains(&path)).unwrap_or(false);
        if is_expanded {
            if let Some(conn) = self.active_connection_mut() {
                conn.expanded_dirs.remove(&path);
            }
            self.remove_children(index);
            cx.notify();
        } else {
            if let Some(conn) = self.active_connection_mut() {
                conn.expanded_dirs.insert(path.clone());
            }
            self.load_directory_children(index, cx);
        }
    }

    fn load_directory_children(
        &mut self,
        parent_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(parent_entry) = self.active_connection().and_then(|c| c.entries.get(parent_index).cloned()) else {
            return;
        };
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let active_key = self.active_host_key.clone();

        let path = parent_entry.entry.path.clone();
        let parent_depth = parent_entry.depth;
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let path_for_load = path.clone();
        cx.spawn(async move |this, cx| {
            let result = tokio_handle
                .spawn(async move { client.list_dir(&path_for_load).await })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r);

            match result {
                Ok(entries) => {
                    this.update(cx, |this, cx| {
                        let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) else {
                            return;
                        };
                        if let Some(parent) = conn.entries.get(parent_index) {
                            if parent.entry.path == path
                                && conn.expanded_dirs.contains(&path)
                            {
                                let sort_mode = this.sort_mode;
                                Self::insert_children_on(
                                    sort_mode,
                                    &mut conn.entries,
                                    parent_index,
                                    entries,
                                    parent_depth + 1,
                                    &path,
                                );
                                // fetch_entry_metadata needs &mut self
                                let _ = conn;
                                this.fetch_entry_metadata(cx);
                                cx.notify();
                            }
                        }
                    })
                    .ok();
                }
                Err(err) => {
                    log::error!("Failed to load directory {}: {}", path, err);
                    this.update(cx, |this, cx| {
                        if let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) {
                            conn.expanded_dirs.remove(&path);
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn insert_children_on(
        sort_mode: SortMode,
        entries: &mut Vec<FileEntry>,
        parent_index: usize,
        new_remote_entries: Vec<RemoteEntry>,
        depth: usize,
        parent_path: &str,
    ) {
        let mut new_entries: Vec<FileEntry> = new_remote_entries
            .into_iter()
            .map(|entry| FileEntry {
                depth,
                parent_path: parent_path.to_string(),
                entry,
            })
            .collect();

        new_entries.sort_by(|a, b| {
            b.entry.is_dir.cmp(&a.entry.is_dir).then_with(|| match sort_mode {
                SortMode::Name => a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()),
                SortMode::ModifiedTime => b.entry.modified.cmp(&a.entry.modified),
            })
        });

        let insert_pos = parent_index + 1;
        for (i, entry) in new_entries.into_iter().enumerate() {
            entries.insert(insert_pos + i, entry);
        }
    }

    fn remove_children(&mut self, parent_index: usize) {
        let Some(conn) = self.active_connection_mut() else {
            return;
        };
        let parent_depth = conn.entries[parent_index].depth;

        let i = parent_index + 1;
        while i < conn.entries.len() {
            if conn.entries[i].depth <= parent_depth {
                break;
            }
            if conn.entries[i].entry.is_dir {
                conn.expanded_dirs.remove(&conn.entries[i].entry.path);
            }
            conn.entries.remove(i);
        }
    }

    fn re_expand_dirs_for_key(&mut self, key: Option<&SshHostKey>, dirs_to_expand: HashSet<String>, cx: &mut Context<Self>) {
        let Some(key) = key else { return; };
        let Some(conn) = self.connections.get_mut(key) else { return; };

        let existing_paths: HashSet<String> = conn
            .entries
            .iter()
            .filter(|e| e.entry.is_dir)
            .map(|e| e.entry.path.clone())
            .collect();

        let valid_dirs: HashSet<String> = dirs_to_expand
            .into_iter()
            .filter(|path| existing_paths.contains(path))
            .collect();

        conn.expanded_dirs = valid_dirs.clone();

        let indices_to_expand: Vec<usize> = conn
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.entry.is_dir && valid_dirs.contains(&entry.entry.path))
            .map(|(i, _)| i)
            .collect();

        // Need to release conn borrow before calling load_directory_children
        let _ = conn;
        for index in indices_to_expand {
            self.load_directory_children(index, cx);
        }
    }

    fn get_parent_dir(&self, path: &str) -> String {
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_else(|| "/".to_string());
        path.rsplit_once('/')
            .map(|(parent, _)| {
                if parent.is_empty() {
                    "/".to_string()
                } else {
                    parent.to_string()
                }
            })
            .unwrap_or(current_path)
    }

    fn start_watch(&mut self, cx: &mut Context<Self>) {
        let watch_enabled = self.active_connection().map(|c| c.watch_enabled).unwrap_or(false);
        let client = self.active_connection().and_then(|c| c.client.clone());
        if !watch_enabled || client.is_none() {
            return;
        }

        if let Some(conn) = self.active_connection_mut() {
            conn.stop_watch();
        }

        let client = client.unwrap();
        let path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_default();
        let active_key = self.active_host_key.clone();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let watch_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WATCH_INTERVAL).await;

                let client_clone = client.clone();
                let path_clone = path.clone();
                let result = tokio_handle
                    .spawn(async move { client_clone.list_dir(&path_clone).await })
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|r| r);

                match result {
                    Ok(new_entries) => {
                        let should_update = this
                            .update(cx, |this, _cx| {
                                let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get(k)) else {
                                    return false;
                                };
                                let old_snapshot: HashMap<String, (u64, Option<u64>)> = conn
                                    .entries
                                    .iter()
                                    .map(|e| {
                                        (e.entry.path.clone(), (e.entry.size, e.entry.modified))
                                    })
                                    .collect();

                                let new_snapshot: HashMap<String, (u64, Option<u64>)> =
                                    new_entries
                                        .iter()
                                        .map(|e| (e.path.clone(), (e.size, e.modified)))
                                        .collect();

                                old_snapshot != new_snapshot
                            })
                            .unwrap_or(false);

                        if should_update {
                            this.update(cx, |this, cx| {
                                let sort_mode = this.sort_mode;
                                let Some(conn) = active_key.as_ref().and_then(|k| this.connections.get_mut(k)) else {
                                    return;
                                };
                                let parent_path = conn.current_path.clone();
                                let expanded_dirs = conn.expanded_dirs.clone();
                                conn.entries = new_entries
                                    .into_iter()
                                    .map(|entry| FileEntry { depth: 0, parent_path: parent_path.clone(), entry })
                                    .collect();
                                Self::sort_entries_on(sort_mode, &mut conn.entries);
                                let _ = conn;
                                this.re_expand_dirs_for_key(active_key.as_ref(), expanded_dirs, cx);
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                    Err(err) => {
                        log::error!("Watch failed to list directory: {}", err);
                    }
                }
            }
        });
        if let Some(conn) = self.active_connection_mut() {
            conn.watch_task = Some(watch_task);
        }
    }

    fn stop_watch(&mut self) {
        if let Some(conn) = self.active_connection_mut() {
            conn.stop_watch();
        }
    }

    fn toggle_watch(&mut self, cx: &mut Context<Self>) {
        if let Some(conn) = self.active_connection_mut() {
            conn.watch_enabled = !conn.watch_enabled;
        }
        let watch_enabled = self.active_connection().map(|c| c.watch_enabled).unwrap_or(false);
        if watch_enabled {
            self.start_watch(cx);
        } else {
            self.stop_watch();
        }
        cx.notify();
    }

    fn visible_entries(&self) -> Vec<(usize, &FileEntry)> {
        let Some(conn) = self.active_connection() else {
            return Vec::new();
        };
        conn.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if !self.show_hidden_files && self.is_hidden_entry(entry) {
                    return false;
                }
                if !self.filter_text.is_empty() {
                    let filter = self.filter_text.to_lowercase();
                    if !entry.entry.name.to_lowercase().contains(&filter) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    fn is_hidden_entry(&self, entry: &FileEntry) -> bool {
        if entry.entry.name.starts_with('.') {
            return true;
        }
        // Also hide children of hidden directories in tree mode
        entry
            .parent_path
            .split('/')
            .any(|segment| segment.starts_with('.'))
    }

    fn navigate_up(&mut self, cx: &mut Context<Self>) {
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_else(|| "/".to_string());
        if current_path == "/" {
            return;
        }
        if let Some(parent) = current_path
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
        {
            if let Some(conn) = self.active_connection_mut() {
                conn.current_path = parent.to_string();
                conn.entries.clear();
                conn.expanded_dirs.clear();
                conn.selected_index = None;
            }
            self.load_directory(cx);
            cx.notify();
        }
    }

    fn start_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            let current_path = self.active_connection().map(|c| c.current_path.as_str()).unwrap_or("/");
            editor.set_text(current_path, window, cx);
            editor.select_all(&editor::actions::SelectAll, window, cx);
            let placeholder = t("sftp_panel.path_placeholder");
            editor.set_placeholder_text(&placeholder, window, cx);
            editor
        });
        cx.subscribe(&editor, |this, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Blurred => {
                    this.path_editor = None;
                    cx.notify();
                }
                _ => {}
            }
        })
        .detach();
        window.focus(&editor.focus_handle(cx), cx);
        self.path_editor = Some(editor);
        cx.notify();
    }

    fn confirm_path_navigation(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.path_editor.as_ref() else {
            return;
        };
        let input = editor.read(cx).text(cx).trim().to_string();
        if input.is_empty() {
            return;
        }
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        self.path_editor = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let resolved = {
                let client = client.clone();
                let input = input.clone();
                tokio_handle
                    .spawn(async move { client.realpath(&input).await })
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|r| r)
            };

            let resolved = match resolved {
                Ok(path) => path,
                Err(err) => {
                    log::error!("Failed to resolve path '{}': {}", input, err);
                    return;
                }
            };

            let stat_result = {
                let client = client.clone();
                let resolved = resolved.clone();
                tokio_handle
                    .spawn(async move { client.stat(&resolved).await })
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|r| r)
            };

            let target_dir = match stat_result {
                Ok(entry) => {
                    if entry.is_dir {
                        resolved
                    } else {
                        // File path — navigate to parent directory
                        resolved
                            .rsplit_once('/')
                            .map(|(p, _)| if p.is_empty() { "/".to_string() } else { p.to_string() })
                            .unwrap_or_else(|| "/".to_string())
                    }
                }
                Err(err) => {
                    log::error!("Failed to stat path '{}': {}", resolved, err);
                    return;
                }
            };

            this.update(cx, |this, cx| {
                if let Some(conn) = this.active_connection_mut() {
                    conn.current_path = target_dir;
                    conn.entries.clear();
                    conn.expanded_dirs.clear();
                    conn.selected_index = None;
                }
                this.load_directory(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn edit_path_action(&mut self, _: &EditPath, window: &mut Window, cx: &mut Context<Self>) {
        self.start_path_edit(window, cx);
    }

    fn navigate_into(&mut self, index: usize, cx: &mut Context<Self>) {
        let path = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| (e.entry.is_dir, e.entry.path.clone()));
        if let Some((true, path)) = path {
            if let Some(conn) = self.active_connection_mut() {
                conn.current_path = path;
                conn.entries.clear();
                conn.expanded_dirs.clear();
                conn.selected_index = None;
            }
            self.load_directory(cx);
            cx.notify();
        }
    }

    fn upload_external_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_default();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);
        let paths = paths.to_vec();

        let task = cx.spawn(async move |this, cx| {
            tokio_handle
                .spawn(async move {
                    for local_path in &paths {
                        client.upload(local_path, &current_path).await?;
                    }
                    anyhow::Ok(())
                })
                .await??;

            this.update(cx, |this, cx| {
                this.load_directory(cx);
            })?;
            anyhow::Ok(())
        });
        task.detach_and_log_err(cx);
    }

    fn on_entry_click(
        &mut self,
        index: usize,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(conn) = self.active_connection_mut() {
            conn.selected_index = Some(index);
        }

        let is_dir = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.is_dir).unwrap_or(false);
        if is_dir {
            self.toggle_expanded(index, cx);
        }

        cx.notify();
    }

    fn on_entry_double_click(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry_info = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.is_dir);
        if let Some(is_dir) = entry_info {
            if is_dir {
                // Double-click navigates into directory (like Open action)
                self.navigate_into(index, cx);
            } else {
                self.open_file(index, window, cx);
            }
        }
    }

    // Action handlers

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible_entries();
        if visible.is_empty() {
            return;
        }
        let current = self.active_connection().and_then(|c| c.selected_index).unwrap_or(0);
        // Find next visible index after current
        let next = visible
            .iter()
            .find(|(real_idx, _)| *real_idx > current)
            .or_else(|| visible.first())
            .map(|(real_idx, _)| *real_idx);
        if let Some(idx) = next {
            if let Some(conn) = self.active_connection_mut() {
                conn.selected_index = Some(idx);
            }
            cx.notify();
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible_entries();
        if visible.is_empty() {
            return;
        }
        let current = self.active_connection().and_then(|c| c.selected_index).unwrap_or(0);
        let prev = visible
            .iter()
            .rev()
            .find(|(real_idx, _)| *real_idx < current)
            .or_else(|| visible.last())
            .map(|(real_idx, _)| *real_idx);
        if let Some(idx) = prev {
            if let Some(conn) = self.active_connection_mut() {
                conn.selected_index = Some(idx);
            }
            cx.notify();
        }
    }

    fn confirm_action(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.path_editor.is_some() {
            self.confirm_path_navigation(window, cx);
            return;
        }
        if self.edit_state.is_some() {
            self.confirm_edit(window, cx);
            return;
        }
        let selected = self.active_connection().and_then(|c| c.selected_index);
        if let Some(index) = selected {
            let is_dir = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.is_dir);
            if let Some(is_dir) = is_dir {
                if is_dir {
                    self.toggle_expanded(index, cx);
                } else {
                    self.open_file(index, window, cx);
                }
            }
        }
    }

    fn go_up_action(&mut self, _: &GoUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_up(cx);
    }

    fn cancel_action(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        if self.path_editor.is_some() {
            self.path_editor = None;
            cx.notify();
            return;
        }
        if self.edit_state.is_some() {
            self.edit_state = None;
            cx.notify();
            return;
        }
        if self.filter_editor.is_some() {
            self.filter_editor = None;
            self.filter_text.clear();
            cx.notify();
            return;
        }
        self.marked_entries.clear();
        if let Some(conn) = self.active_connection_mut() {
            conn.selected_index = None;
        }
        cx.notify();
    }

    fn toggle_sort_mode(&mut self, _: &ToggleSortMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.sort_mode = match self.sort_mode {
            SortMode::Name => SortMode::ModifiedTime,
            SortMode::ModifiedTime => SortMode::Name,
        };
        self.sort_entries();
        cx.notify();
    }

    fn toggle_show_hidden_files(&mut self, _: &ToggleShowHiddenFiles, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden_files = !self.show_hidden_files;
        cx.notify();
    }

    fn copy_path_action(&mut self, _: &CopyPath, _window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.active_connection().and_then(|c| c.selected_index);
        if let Some(index) = selected {
            if let Some(path) = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.path.clone()) {
                cx.write_to_clipboard(ClipboardItem::new_string(path));
            }
        }
    }

    fn cut_action(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if !paths.is_empty() {
            self.clipboard = Some(ClipboardState {
                paths,
                is_cut: true,
            });
            cx.notify();
        }
    }

    fn copy_action(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if !paths.is_empty() {
            self.clipboard = Some(ClipboardState {
                paths,
                is_cut: false,
            });
            cx.notify();
        }
    }

    fn selected_paths(&self) -> Vec<String> {
        if !self.marked_entries.is_empty() {
            self.marked_entries
                .iter()
                .filter_map(|&idx| self.active_connection().and_then(|c| c.entries.get(idx)).map(|e| e.entry.path.clone()))
                .collect()
        } else if let Some(index) = self.active_connection().and_then(|c| c.selected_index) {
            self.active_connection()
                .and_then(|c| c.entries.get(index))
                .map(|e| vec![e.entry.path.clone()])
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn paste_action(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = self.clipboard.clone() else {
            return;
        };
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_default();
        let is_cut = clipboard.is_cut;
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let task = cx.spawn(async move |this, cx| {
            tokio_handle
                .spawn(async move {
                    for source_path in &clipboard.paths {
                        let name = source_path
                            .rsplit_once('/')
                            .map(|(_, n)| n)
                            .unwrap_or(source_path);
                        let dest = format!(
                            "{}/{}",
                            current_path.trim_end_matches('/'),
                            name
                        );
                        if is_cut {
                            client.rename(source_path, &dest).await?;
                        } else {
                            // Copy via read+write for files
                            let source_dir = source_path
                                .rsplit_once('/')
                                .map(|(dir, _)| dir)
                                .unwrap_or("");
                            let temp_dir = std::env::temp_dir()
                                .join("bspterm_sftp_copy")
                                .join(sanitize_remote_dir_to_path(source_dir));
                            std::fs::create_dir_all(&temp_dir)?;
                            let temp_local = temp_dir.join(name);
                            client.recursive_download(source_path, &temp_local).await?;
                            client.upload(&temp_local, &current_path).await?;
                            let _ = std::fs::remove_dir_all(&temp_dir);
                        }
                    }
                    anyhow::Ok(())
                })
                .await??;

            this.update(cx, |this, cx| {
                if is_cut {
                    this.clipboard = None;
                }
                this.load_directory(cx);
            })?;
            anyhow::Ok(())
        });
        task.detach_and_log_err(cx);
    }

    fn new_file_action(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_entry(false, window, cx);
    }

    fn new_directory_action(&mut self, _: &NewDirectory, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_entry(true, window, cx);
    }

    fn start_new_entry(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            let placeholder = if is_dir {
                t("sftp_panel.new_directory_placeholder")
            } else {
                t("sftp_panel.new_file_placeholder")
            };
            editor.set_placeholder_text(&placeholder, window, cx);
            editor
        });
        let subscription = cx.subscribe(&editor, |this, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Blurred => {
                    this.edit_state = None;
                    cx.notify();
                }
                _ => {}
            }
        });
        window.focus(&editor.focus_handle(cx), cx);
        self.edit_state = Some(EditState {
            index: None,
            is_dir,
            editor,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn rename_action(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active_connection().and_then(|c| c.selected_index) else {
            return;
        };
        let Some((name, is_dir)) = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| (e.entry.name.clone(), e.entry.is_dir)) else {
            return;
        };

        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(name.clone(), window, cx);
            editor
        });
        let subscription = cx.subscribe(&editor, |this, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Blurred => {
                    this.edit_state = None;
                    cx.notify();
                }
                _ => {}
            }
        });
        window.focus(&editor.focus_handle(cx), cx);
        self.edit_state = Some(EditState {
            index: Some(index),
            is_dir,
            editor,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn confirm_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit_state) = self.edit_state.take() else {
            return;
        };
        let new_name = edit_state.editor.read(cx).text(cx).trim().to_string();
        if new_name.is_empty() {
            cx.notify();
            return;
        }

        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let current_path = self.active_connection().map(|c| c.current_path.clone()).unwrap_or_default();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        if let Some(index) = edit_state.index {
            // Rename existing entry
            let Some((old_path, parent_path)) = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| (e.entry.path.clone(), e.parent_path.clone())) else {
                return;
            };
            let new_path = format!(
                "{}/{}",
                parent_path.trim_end_matches('/'),
                new_name
            );
            let task = cx.spawn(async move |this, cx| {
                tokio_handle
                    .spawn(async move { client.rename(&old_path, &new_path).await })
                    .await??;
                this.update(cx, |this, cx| {
                    this.load_directory(cx);
                })?;
                anyhow::Ok(())
            });
            task.detach_and_log_err(cx);
        } else {
            // Create new entry
            let new_path = format!(
                "{}/{}",
                current_path.trim_end_matches('/'),
                new_name
            );
            let is_dir = edit_state.is_dir;
            let task = cx.spawn(async move |this, cx| {
                tokio_handle
                    .spawn(async move {
                        if is_dir {
                            client.mkdir(&new_path).await
                        } else {
                            client.create_empty_file(&new_path).await
                        }
                    })
                    .await??;
                this.update(cx, |this, cx| {
                    this.load_directory(cx);
                })?;
                anyhow::Ok(())
            });
            task.detach_and_log_err(cx);
        }
        cx.notify();
    }

    fn delete_action(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }

        let count = paths.len();
        let message = if count == 1 {
            let name = paths[0]
                .rsplit_once('/')
                .map(|(_, n)| n)
                .unwrap_or(&paths[0]);
            format!("{} \"{}\"?", t("sftp_panel.confirm_delete"), name)
        } else {
            format!("{} {} {}?", t("sftp_panel.confirm_delete"), count, t("sftp_panel.items"))
        };

        let answer = window.prompt(PromptLevel::Warning, &message, None, &["OK", "Cancel"], cx);
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let entries = self.active_connection().map(|c| c.entries.clone()).unwrap_or_default();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn(async move |this, cx| {
            if answer.await? == 0 {
                tokio_handle
                    .spawn(async move {
                        for path in &paths {
                            let is_dir = entries
                                .iter()
                                .find(|e| &e.entry.path == path)
                                .map(|e| e.entry.is_dir)
                                .unwrap_or(false);
                            if is_dir {
                                client.recursive_remove(path).await?;
                            } else {
                                client.remove_file(path).await?;
                            }
                        }
                        anyhow::Ok(())
                    })
                    .await??;
                this.update(cx, |this, cx| {
                    this.marked_entries.clear();
                    if let Some(conn) = this.active_connection_mut() {
                        conn.selected_index = None;
                    }
                    this.load_directory(cx);
                })?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn open_action(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.active_connection().and_then(|c| c.selected_index);
        if let Some(index) = selected {
            let is_dir = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.is_dir);
            if let Some(is_dir) = is_dir {
                if is_dir {
                    self.navigate_into(index, cx);
                } else {
                    self.open_file(index, window, cx);
                }
            }
        }
    }

    fn open_file(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((remote_path, name)) = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| (e.entry.path.clone(), e.entry.name.clone())) else {
            return;
        };
        let entry_is_dir = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| e.entry.is_dir).unwrap_or(false);
        if entry_is_dir {
            return;
        }
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let host = self
            .active_host_key
            .as_ref()
            .map(|k| {
                let user_part = k.username.as_deref().unwrap_or("anon");
                format!("{}@{}_{}", user_part, k.host, k.port)
            })
            .unwrap_or_else(|| "unknown".to_string());
        let workspace = self.workspace.clone();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn_in(window, async move |this, cx| {
            let remote_dir = remote_path
                .rsplit_once('/')
                .map(|(dir, _)| dir)
                .unwrap_or("");
            let temp_dir = std::env::temp_dir()
                .join("bspterm_sftp")
                .join(&host)
                .join(sanitize_remote_dir_to_path(remote_dir));
            std::fs::create_dir_all(&temp_dir)?;
            let local_path = temp_dir.join(&name);

            let client_for_download = client.clone();
            let remote_for_download = remote_path.clone();
            let local_for_download = local_path.clone();
            tokio_handle
                .spawn(async move {
                    client_for_download
                        .read_file(&remote_for_download, &local_for_download)
                        .await
                })
                .await??;

            // Compute original md5 and get mtime for conflict detection
            let file_bytes = std::fs::read(&local_path)?;
            let original_md5 = md5::compute(&file_bytes).0;

            let client_for_stat = client.clone();
            let remote_for_stat = remote_path.clone();
            let original_mtime = tokio_handle
                .spawn(async move { client_for_stat.stat(&remote_for_stat).await })
                .await?
                .ok()
                .and_then(|entry| entry.modified);

            let mapping = SftpFileMapping {
                remote_path: remote_path.clone(),
                client: client.clone(),
                original_mtime,
                original_md5,
            };

            let local_path_for_map = local_path.clone();
            let item = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_abs_path(
                        local_path.clone(),
                        workspace::OpenOptions::default(),
                        window,
                        cx,
                    )
                })?
                .await?;

            this.update_in(cx, |this, window, cx| {
                // Only register mapping if not already tracked
                if this.sftp_file_mappings.contains_key(&local_path_for_map) {
                    return;
                }
                this.sftp_file_mappings
                    .insert(local_path_for_map.clone(), mapping);

                if let Some(editor) = item.downcast::<Editor>() {
                    let local_path_for_save = local_path_for_map.clone();
                    let subscription =
                        cx.subscribe_in(&editor, window, move |this, _editor, event, window, cx| {
                            if matches!(event, EditorEvent::Saved) {
                                this.upload_on_save(local_path_for_save.clone(), window, cx);
                            }
                        });
                    this._subscriptions.push(subscription);
                }
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn upload_on_save(
        &mut self,
        local_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mapping) = self.sftp_file_mappings.get(&local_path) else {
            return;
        };
        let client = mapping.client.clone();
        let remote_path = mapping.remote_path.clone();
        let original_mtime = mapping.original_mtime;
        let original_md5 = mapping.original_md5;
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn_in(window, async move |this, cx| {
            // Check remote mtime
            let client_for_stat = client.clone();
            let remote_for_stat = remote_path.clone();
            let stat_result = tokio_handle
                .spawn(async move { client_for_stat.stat(&remote_for_stat).await })
                .await?;

            let needs_conflict_check = match &stat_result {
                Ok(entry) => entry.modified != original_mtime,
                Err(_) => false, // File may have been deleted; just upload
            };

            if needs_conflict_check {
                // mtime differs — check md5
                let client_for_read = client.clone();
                let remote_for_read = remote_path.clone();
                let remote_bytes = tokio_handle
                    .spawn(async move { client_for_read.read_file_bytes(&remote_for_read).await })
                    .await??;
                let current_md5 = md5::compute(&remote_bytes).0;

                if current_md5 != original_md5 {
                    // Content changed — ask user
                    let answer = this
                        .update_in(cx, |_this, window, cx| {
                            window.prompt(
                                PromptLevel::Warning,
                                "远端文件已被修改，是否覆盖上传？",
                                None,
                                &["覆盖上传", "取消"],
                                cx,
                            )
                        })?
                        .await?;

                    if answer != 0 {
                        return anyhow::Ok(());
                    }
                }
            }

            // Upload
            let client_for_upload = client.clone();
            let remote_for_upload = remote_path.clone();
            let local_for_upload = local_path.clone();
            tokio_handle
                .spawn(async move {
                    client_for_upload
                        .write_file(&local_for_upload, &remote_for_upload)
                        .await
                })
                .await??;

            // Update baseline mtime and md5
            let client_for_stat2 = client.clone();
            let remote_for_stat2 = remote_path.clone();
            let new_mtime = tokio_handle
                .spawn(async move { client_for_stat2.stat(&remote_for_stat2).await })
                .await?
                .ok()
                .and_then(|entry| entry.modified);

            let new_bytes = std::fs::read(&local_path)?;
            let new_md5 = md5::compute(&new_bytes).0;

            this.update(cx, |this, cx| {
                if let Some(mapping) = this.sftp_file_mappings.get_mut(&local_path) {
                    mapping.original_mtime = new_mtime;
                    mapping.original_md5 = new_md5;
                }
                this.load_directory(cx);
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn download_action(&mut self, _: &Download, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let entries = self.active_connection().map(|c| c.entries.clone()).unwrap_or_default();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);
        let workspace = self.workspace.clone();

        let download_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(|h| PathBuf::from(h).join("Downloads"))
            .unwrap_or_else(|_| std::env::temp_dir().join("Downloads"));
        let download_dir_display = download_dir.display().to_string();

        let task = cx.spawn(async move |_this, cx| {
            std::fs::create_dir_all(&download_dir)?;
            tokio_handle
                .spawn(async move {
                    for remote_path in &paths {
                        let name = remote_path
                            .rsplit_once('/')
                            .map(|(_, n)| n)
                            .unwrap_or(remote_path);
                        let local_path = unique_download_path(&download_dir, name);
                        let is_dir = entries
                            .iter()
                            .find(|e| &e.entry.path == remote_path)
                            .map(|e| e.entry.is_dir)
                            .unwrap_or(false);
                        if is_dir {
                            client
                                .recursive_download(remote_path, &local_path)
                                .await?;
                        } else {
                            client.read_file(remote_path, &local_path).await?;
                        }
                    }
                    anyhow::Ok(())
                })
                .await??;
            workspace
                .update(cx, |workspace, cx| {
                    let toast = Toast::new(
                        NotificationId::unique::<SftpPanel>(),
                        format!("Downloaded to {}", download_dir_display),
                    )
                    .autohide();
                    workspace.show_toast(toast, cx);
                })
                .ok();
            anyhow::Ok(())
        });
        task.detach_and_log_err(cx);
    }

    fn chmod_action(&mut self, _: &Chmod, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active_connection().and_then(|c| c.selected_index) else {
            return;
        };
        let Some((current_mode, remote_path)) = self.active_connection().and_then(|c| c.entries.get(index)).map(|e| (e.entry.permissions & 0o7777, e.entry.path.clone())) else {
            return;
        };
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        // For now, toggle executable bit as a simple chmod action
        let new_mode = if current_mode & 0o111 != 0 {
            current_mode & !0o111 // Remove execute
        } else {
            current_mode | 0o111 // Add execute
        };

        let task = cx.spawn(async move |this, cx| {
            tokio_handle
                .spawn(async move { client.chmod(&remote_path, new_mode).await })
                .await??;
            this.update(cx, |this, cx| {
                this.load_directory(cx);
            })?;
            anyhow::Ok(())
        });
        task.detach_and_log_err(cx);
    }

    fn toggle_filter_action(&mut self, _: &ToggleFilter, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_editor.is_some() {
            self.filter_editor = None;
            self.filter_text.clear();
            cx.notify();
            return;
        }
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            let placeholder = t("sftp_panel.filter_placeholder");
            editor.set_placeholder_text(&placeholder, window, cx);
            editor
        });
        let entity = cx.entity();
        cx.subscribe(&editor, move |_this, editor, event: &EditorEvent, cx| {
            match event {
                EditorEvent::BufferEdited { .. } => {
                    let text = editor.read(cx).text(cx);
                    entity.update(cx, |this, cx| {
                        this.filter_text = text;
                        cx.notify();
                    });
                }
                _ => {}
            }
        })
        .detach();
        window.focus(&editor.focus_handle(cx), cx);
        self.filter_editor = Some(editor);
        cx.notify();
    }

    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = index {
            if !self.marked_entries.contains(&idx) {
                self.marked_entries.clear();
            }
            if let Some(conn) = self.active_connection_mut() {
                conn.selected_index = Some(idx);
            }
        }

        let is_dir = index
            .and_then(|idx| self.active_connection().and_then(|c| c.entries.get(idx)))
            .map(|e| e.entry.is_dir)
            .unwrap_or(false);
        let has_selection = index.is_some();
        let has_clipboard = self.clipboard.is_some();
        let has_multi_select = !self.marked_entries.is_empty();
        let focus = self.focus_handle.clone();

        let context_menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let menu = menu.context(focus);
            if has_multi_select {
                menu.action(t("sftp_panel.download"), Box::new(Download))
                    .action(t("sftp_panel.delete"), Box::new(Delete))
            } else if has_selection {
                let menu = menu
                    .action(t("sftp_panel.open"), Box::new(Open))
                    .action(t("sftp_panel.download"), Box::new(Download))
                    .separator()
                    .action(t("sftp_panel.rename"), Box::new(Rename))
                    .action(t("sftp_panel.copy_path"), Box::new(CopyPath))
                    .separator()
                    .action(t("sftp_panel.cut"), Box::new(Cut))
                    .action(t("sftp_panel.copy"), Box::new(Copy));
                let menu = if is_dir && has_clipboard {
                    menu.action(t("sftp_panel.paste"), Box::new(Paste))
                } else {
                    menu
                };
                menu.separator()
                    .action(t("sftp_panel.delete"), Box::new(Delete))
                    .separator()
                    .action(t("sftp_panel.chmod"), Box::new(Chmod))
            } else {
                let menu = menu
                    .action(t("sftp_panel.new_file"), Box::new(NewFile))
                    .action(t("sftp_panel.new_directory"), Box::new(NewDirectory));
                let menu = if has_clipboard {
                    menu.separator()
                        .action(t("sftp_panel.paste"), Box::new(Paste))
                } else {
                    menu
                };
                menu.separator()
                    .action(t("sftp_panel.refresh"), Box::new(RefreshDirectory))
            }
        });

        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu = None;
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    // Drag-and-drop

    #[allow(clippy::too_many_arguments)]
    fn handle_drag_move(
        &mut self,
        target_index: usize,
        target_path: &str,
        target_is_dir: bool,
        target_is_expanded: bool,
        dragged_path: &str,
        mouse_y: f32,
        item_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mouse_y < 0.0 || mouse_y > item_height {
            return;
        }

        // Don't drop onto self
        if dragged_path == target_path {
            self.drag_target = None;
            self.hover_expand_task = None;
            cx.notify();
            return;
        }

        // Don't drop into own descendant
        if target_path.starts_with(dragged_path)
            && target_path.as_bytes().get(dragged_path.len()) == Some(&b'/')
        {
            self.drag_target = None;
            self.hover_expand_task = None;
            cx.notify();
            return;
        }

        let relative_y = mouse_y / item_height;

        let new_target = if target_is_dir {
            if relative_y < 0.25 {
                SftpDragTarget::BeforeEntry {
                    path: target_path.to_string(),
                }
            } else {
                // Bottom 75% of directory → drop into it
                SftpDragTarget::IntoDir {
                    path: target_path.to_string(),
                }
            }
        } else if relative_y < 0.5 {
            SftpDragTarget::BeforeEntry {
                path: target_path.to_string(),
            }
        } else {
            SftpDragTarget::AfterEntry {
                path: target_path.to_string(),
            }
        };

        let target_changed = self.drag_target.as_ref() != Some(&new_target);
        self.drag_target = Some(new_target.clone());

        if target_changed {
            self.hover_expand_task = None;

            if let SftpDragTarget::IntoDir { .. } = &new_target {
                if target_is_dir && !target_is_expanded {
                    let target_index = target_index;
                    self.hover_expand_task =
                        Some(cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(500))
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                this.toggle_expanded(target_index, cx);
                            });
                        }));
                }
            }

            cx.notify();
        }
    }

    fn handle_internal_drop(
        &mut self,
        dragged: &DraggedSftpEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.drag_target.take() else {
            return;
        };
        self.hover_expand_task = None;

        let target_dir = match target {
            SftpDragTarget::IntoDir { path } => path,
            SftpDragTarget::BeforeEntry { ref path }
            | SftpDragTarget::AfterEntry { ref path } => self.get_parent_dir(path),
        };

        let source_path = dragged.path.clone();
        let source_name = dragged.name.clone();
        let dest_path = format!(
            "{}/{}",
            target_dir.trim_end_matches('/'),
            source_name
        );

        // Don't move to same location
        if source_path == dest_path {
            cx.notify();
            return;
        }

        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let task = cx.spawn(async move |this, cx| {
            tokio_handle
                .spawn(async move { client.rename(&source_path, &dest_path).await })
                .await??;
            this.update(cx, |this, cx| {
                this.load_directory(cx);
            })?;
            anyhow::Ok(())
        });
        task.detach_and_log_err(cx);
    }

    // Rendering

    fn render_title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.active_connection().map(|c| c.status.clone()).unwrap_or(ConnectionStatus::Disconnected);
        let watch_enabled = self.active_connection().map(|c| c.watch_enabled).unwrap_or(false);
        let status_text = match &status {
            ConnectionStatus::Disconnected => t("sftp_panel.disconnected"),
            ConnectionStatus::Connecting => t("sftp_panel.connecting"),
            ConnectionStatus::Connected => t("sftp_panel.connected"),
            ConnectionStatus::Error(_) => t("sftp_panel.error"),
        };

        self.panel_header_container(window, cx)
            .px_2()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .justify_between()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Folder)
                            .color(Color::Muted)
                            .size(IconSize::Small),
                    )
                    .child(Label::new("SFTP").size(LabelSize::Small))
                    .child(
                        Label::new(format!("({})", status_text))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .when(status == ConnectionStatus::Connected, |this| {
                        let current_sort = self.sort_mode;
                        let weak = cx.weak_entity();
                        this.child(
                            PopoverMenu::new("sftp-sort-popover")
                                .trigger(
                                    IconButton::new("sftp-sort", IconName::ListFilter)
                                        .icon_size(IconSize::Small)
                                        .tooltip(Tooltip::text(t("sftp_panel.sort_by"))),
                                )
                                .menu(move |window, cx| {
                                    let weak = weak.clone();
                                    let weak2 = weak.clone();
                                    Some(ContextMenu::build(
                                        window,
                                        cx,
                                        move |menu, _window, _cx| {
                                            menu.toggleable_entry(
                                                t("sftp_panel.sort_by_name"),
                                                current_sort == SortMode::Name,
                                                IconPosition::Start,
                                                None,
                                                {
                                                    let weak = weak.clone();
                                                    move |_window, cx| {
                                                        weak.update(cx, |this, cx| {
                                                            this.sort_mode = SortMode::Name;
                                                            this.sort_entries();
                                                            cx.notify();
                                                        })
                                                        .ok();
                                                    }
                                                },
                                            )
                                            .toggleable_entry(
                                                t("sftp_panel.sort_by_modified"),
                                                current_sort == SortMode::ModifiedTime,
                                                IconPosition::Start,
                                                None,
                                                move |_window, cx| {
                                                    weak2
                                                        .update(cx, |this, cx| {
                                                            this.sort_mode =
                                                                SortMode::ModifiedTime;
                                                            this.sort_entries();
                                                            cx.notify();
                                                        })
                                                        .ok();
                                                },
                                            )
                                        },
                                    ))
                                })
                                .anchor(gpui::Corner::TopRight),
                        )
                        .child(
                            IconButton::new("sftp-hidden", if self.show_hidden_files { IconName::Eye } else { IconName::Close })
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(if self.show_hidden_files {
                                    t("sftp_panel.hide_hidden_files")
                                } else {
                                    t("sftp_panel.show_hidden_files")
                                }))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.show_hidden_files = !this.show_hidden_files;
                                    cx.notify();
                                })),
                        )
                        .child(
                            IconButton::new("sftp-watch", if watch_enabled { IconName::Eye } else { IconName::Close })
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(if watch_enabled {
                                    t("sftp_panel.watch_stop")
                                } else {
                                    t("sftp_panel.watch_start")
                                }))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_watch(cx);
                                })),
                        )
                        .child(
                            IconButton::new("sftp-refresh", IconName::ArrowCircle)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(t("sftp_panel.refresh")))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.sync_from_terminal(cx);
                                })),
                        )
                    })
                    .child(match &status {
                        ConnectionStatus::Connected => {
                            IconButton::new("sftp-connect", IconName::Disconnected)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(t("sftp_panel.disconnect")))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.disconnect(cx);
                                }))
                        }
                        ConnectionStatus::Connecting => {
                            IconButton::new("sftp-connect", IconName::Link)
                                .icon_size(IconSize::Small)
                                .disabled(true)
                                .tooltip(Tooltip::text(t("sftp_panel.connecting")))
                        }
                        _ => {
                            IconButton::new("sftp-connect", IconName::Link)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(t("sftp_panel.connect_from_terminal")))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.connect_from_active_terminal(cx);
                                }))
                        }
                    }),
            )
    }

    fn render_breadcrumb(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                IconButton::new("sftp-nav-up", IconName::ArrowUp)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text(t("sftp_panel.navigate_up")))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.navigate_up(cx);
                    })),
            )
            .when_some(self.path_editor.clone(), |el, editor| {
                el.child(
                    h_flex()
                        .flex_grow()
                        .min_w_0()
                        .child(editor),
                )
            })
            .when(self.path_editor.is_none(), |el| {
                el.child(
                    div()
                        .id("sftp-path-label")
                        .cursor_pointer()
                        .flex_grow()
                        .min_w_0()
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.start_path_edit(window, cx);
                        }))
                        .child(
                            Label::new(self.active_connection().map(|c| c.current_path.clone()).unwrap_or_else(|| "/".to_string()))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
            })
    }

    fn render_entries(
        &self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|display_ix| {
                let real_ix = self
                    .cached_visible_indices
                    .get(display_ix)
                    .copied()
                    .unwrap_or(display_ix);
                self.render_entry(real_ix, cx)
            })
            .collect()
    }

    fn fetch_entry_metadata(&mut self, cx: &mut Context<Self>) {
        self.fetch_md5_for_entries(cx);
        self.fetch_dir_item_counts(cx);
    }

    fn fetch_md5_for_entries(&mut self, cx: &mut Context<Self>) {
        let md5_available = self.active_connection().and_then(|c| c.md5_available);
        if md5_available == Some(false) {
            return;
        }

        let Some(host_key) = self.active_host_key.clone() else {
            return;
        };
        let Some(session) = self.sftp_store.read(cx).get_session(&host_key) else {
            return;
        };

        let file_paths: Vec<String> = self.active_connection()
            .map(|c| c.entries.iter()
                .filter(|e| !e.entry.is_dir && !c.md5_cache.contains_key(&e.entry.path))
                .map(|e| e.entry.path.clone())
                .collect())
            .unwrap_or_default();

        if file_paths.is_empty() {
            return;
        }

        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let md5_task = cx.spawn(async move |this, cx| {
            // Check md5sum availability if not yet determined
            if md5_available.is_none() {
                let session_check = session.clone();
                let available = tokio_handle
                    .spawn(async move {
                        session_check
                            .exec_command("command -v md5sum")
                            .await
                            .is_ok()
                    })
                    .await
                    .unwrap_or(false);

                this.update(cx, |this, _cx| {
                    if let Some(conn) = this.active_connection_mut() {
                        conn.md5_available = Some(available);
                    }
                })
                .ok();

                if !available {
                    return;
                }
            }

            // Batch files, max 50 per command
            for chunk in file_paths.chunks(50) {
                let escaped: Vec<String> = chunk
                    .iter()
                    .map(|p| format!("'{}'", p.replace('\'', "'\\''")))
                    .collect();
                let command = format!("md5sum {}", escaped.join(" "));

                let session_exec = session.clone();
                let result = tokio_handle
                    .spawn(async move { session_exec.exec_command(&command).await })
                    .await;

                if let Ok(Ok(output)) = result {
                    let mut batch: HashMap<String, String> = HashMap::new();
                    for line in output.lines() {
                        // Format: "<32hex>  <filename>" or "<32hex> <filename>"
                        if let Some((hash, _path)) = line.split_once(|c: char| c.is_whitespace()) {
                            let hash = hash.trim();
                            if hash.len() == 32
                                && hash.chars().all(|c| c.is_ascii_hexdigit())
                            {
                                // Extract the file path (skip leading whitespace after hash)
                                let file_path = line[hash.len()..].trim_start();
                                if !file_path.is_empty() {
                                    batch.insert(file_path.to_string(), hash.to_string());
                                }
                            }
                        }
                    }

                    if !batch.is_empty() {
                        this.update(cx, |this, cx| {
                            if let Some(conn) = this.active_connection_mut() {
                                conn.md5_cache.extend(batch);
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                }
            }
        });
        if let Some(conn) = self.active_connection_mut() {
            conn.md5_task = Some(md5_task);
        }
    }

    fn fetch_dir_item_counts(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.active_connection().and_then(|c| c.client.clone()) else {
            return;
        };

        let dir_paths: Vec<String> = self.active_connection()
            .map(|c| c.entries.iter()
                .filter(|e| e.entry.is_dir && !c.dir_count_cache.contains_key(&e.entry.path))
                .map(|e| e.entry.path.clone())
                .collect())
            .unwrap_or_default();

        if dir_paths.is_empty() {
            return;
        }

        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        let dir_count_task = cx.spawn(async move |this, cx| {
            let mut counts = Vec::new();
            for path in dir_paths {
                let client = client.clone();
                let path_clone = path.clone();
                let result = tokio_handle
                    .spawn(async move { client.list_dir(&path_clone).await })
                    .await;
                if let Ok(Ok(entries)) = result {
                    counts.push((path, entries.len()));
                }
            }

            if !counts.is_empty() {
                this.update(cx, |this, cx| {
                    if let Some(conn) = this.active_connection_mut() {
                        for (path, count) in counts {
                            conn.dir_count_cache.insert(path, count);
                        }
                    }
                    cx.notify();
                })
                .ok();
            }
        });
        if let Some(conn) = self.active_connection_mut() {
            conn.dir_count_task = Some(dir_count_task);
        }
    }

    fn render_entry(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(conn) = self.active_connection() else {
            return gpui::Empty.into_any();
        };
        let Some(file_entry) = conn.entries.get(index) else {
            return gpui::Empty.into_any();
        };

        let entry = &file_entry.entry;
        let is_selected = conn.selected_index == Some(index);
        let is_expanded = conn.expanded_dirs.contains(&entry.path);
        let is_dir = entry.is_dir;
        let depth = file_entry.depth;
        let entry_path = entry.path.clone();

        let icon = if is_dir {
            if is_expanded {
                IconName::FolderOpen
            } else {
                IconName::Folder
            }
        } else {
            IconName::File
        };

        let is_editing = self
            .edit_state
            .as_ref()
            .is_some_and(|s| s.index == Some(index));

        if is_editing {
            let editor = self.edit_state.as_ref().map(|s| s.editor.clone()).unwrap();
            return ListItem::new(("sftp-entry", index))
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(true)
                .indent_level(depth)
                .indent_step_size(px(12.))
                .start_slot(Icon::new(icon).size(IconSize::Small))
                .child(editor)
                .into_any_element();
        }

        let size_label = if !is_dir {
            format_file_size(entry.size)
        } else {
            String::new()
        };

        let md5_suffix = if !is_dir {
            conn.md5_cache
                .get(&entry.path)
                .map(|hash| hash[hash.len().saturating_sub(5)..].to_string())
        } else {
            None
        };

        let dir_count_label = if is_dir {
            conn.dir_count_cache
                .get(&entry.path)
                .map(|count| t("sftp_panel.items").replace("{}", &count.to_string()))
        } else {
            None
        };

        // Drag-and-drop visual indicators
        let show_before_indicator = matches!(
            &self.drag_target,
            Some(SftpDragTarget::BeforeEntry { path }) if *path == entry_path
        );
        let show_after_indicator = matches!(
            &self.drag_target,
            Some(SftpDragTarget::AfterEntry { path }) if *path == entry_path
        );
        let show_into_highlight = matches!(
            &self.drag_target,
            Some(SftpDragTarget::IntoDir { path }) if *path == entry_path
        );

        let accent_color = cx.theme().colors().text_accent;
        let drop_bg = cx.theme().colors().drop_target_background;

        let drag_data = DraggedSftpEntry {
            path: entry.path.clone(),
            name: entry.name.clone(),
            is_dir,
        };

        let list_item = ListItem::new(("sftp-entry", index))
            .spacing(ListItemSpacing::Sparse)
            .toggle_state(is_selected)
            .indent_level(depth)
            .indent_step_size(px(12.))
            .when(is_dir, |this| {
                this.toggle(Some(is_expanded))
                    .on_toggle(cx.listener(move |this, _, _window, cx| {
                        this.toggle_expanded(index, cx);
                    }))
            })
            .start_slot(Icon::new(icon).size(IconSize::Small))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        Label::new(entry.name.clone())
                            .size(LabelSize::Small)
                            .single_line(),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .when(!size_label.is_empty(), |el| {
                                el.child(
                                    div().w(px(60.)).flex().justify_end().child(
                                        Label::new(size_label)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                                )
                            })
                            .when_some(md5_suffix, |el, suffix| {
                                el.child(
                                    div().w(px(38.)).child(
                                        Label::new(suffix)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                                )
                            })
                            .when_some(dir_count_label, |el, count| {
                                el.child(
                                    Label::new(count)
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                )
                            }),
                    ),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let is_double_click = matches!(event, ClickEvent::Mouse(mouse) if mouse.down.click_count == 2);
                if is_double_click {
                    this.on_entry_double_click(index, window, cx);
                } else {
                    this.on_entry_click(index, event, window, cx);
                }
            }))
            .on_secondary_mouse_down(cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.deploy_context_menu(event.position, Some(index), window, cx);
            }));

        let entry_wrapper = div()
            .id(SharedString::from(format!("sftp-entry-wrapper-{}", index)))
            .w_full()
            .when(show_into_highlight, |this| {
                this.bg(drop_bg).border_l_2().border_color(accent_color)
            })
            .on_drag(drag_data, move |drag_data, _click_offset, _window, cx| {
                cx.new(|_| DraggedSftpEntryView {
                    name: drag_data.name.clone(),
                    is_dir: drag_data.is_dir,
                })
            })
            .on_drag_move::<DraggedSftpEntry>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSftpEntry>, window, cx| {
                    let bounds = event.bounds;
                    let mouse_y = event.event.position.y - bounds.origin.y;
                    let item_height = bounds.size.height;
                    let dragged_path = event.drag(cx).path.clone();
                    this.handle_drag_move(
                        index,
                        &entry_path,
                        is_dir,
                        is_expanded,
                        &dragged_path,
                        mouse_y.into(),
                        item_height.into(),
                        window,
                        cx,
                    );
                },
            ))
            .on_drop(cx.listener(
                move |this, dragged: &DraggedSftpEntry, window, cx| {
                    this.handle_internal_drop(dragged, window, cx);
                },
            ))
            .child(list_item);

        let before_line = div()
            .w_full()
            .h(px(2.))
            .when(show_before_indicator, |this| this.bg(accent_color));

        let after_line = div()
            .w_full()
            .h(px(2.))
            .when(show_after_indicator, |this| this.bg(accent_color));

        v_flex()
            .w_full()
            .child(before_line)
            .child(entry_wrapper)
            .child(after_line)
            .into_any_element()
    }

    fn render_empty_state(&self) -> impl IntoElement {
        let status = self.active_connection().map(|c| c.status.clone()).unwrap_or(ConnectionStatus::Disconnected);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .p_4()
            .gap_2()
            .child(
                Icon::new(IconName::Folder)
                    .size(IconSize::Medium)
                    .color(Color::Muted),
            )
            .child(match &status {
                ConnectionStatus::Disconnected => Label::new(t("sftp_panel.click_connect"))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
                ConnectionStatus::Connecting => Label::new(t("sftp_panel.connecting"))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
                ConnectionStatus::Connected => Label::new(t("sftp_panel.empty_directory"))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
                ConnectionStatus::Error(msg) => Label::new(msg.clone())
                    .size(LabelSize::Small)
                    .color(Color::Error)
                    .into_any_element(),
            })
    }
}

impl EventEmitter<PanelEvent> for SftpPanel {}
impl PanelHeader for SftpPanel {}

impl Render for SftpPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.cached_visible_indices = self
            .visible_entries()
            .into_iter()
            .map(|(idx, _)| idx)
            .collect();
        let item_count = self.cached_visible_indices.len();
        let is_connected = self.active_connection().map(|c| c.status == ConnectionStatus::Connected).unwrap_or(false);

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .key_context("SftpPanel")
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm_action))
            .on_action(cx.listener(Self::go_up_action))
            .on_action(cx.listener(Self::cancel_action))
            .on_action(cx.listener(Self::toggle_sort_mode))
            .on_action(cx.listener(Self::toggle_show_hidden_files))
            .on_action(cx.listener(Self::copy_path_action))
            .on_action(cx.listener(Self::cut_action))
            .on_action(cx.listener(Self::copy_action))
            .on_action(cx.listener(Self::paste_action))
            .on_action(cx.listener(Self::delete_action))
            .on_action(cx.listener(Self::new_file_action))
            .on_action(cx.listener(Self::new_directory_action))
            .on_action(cx.listener(Self::rename_action))
            .on_action(cx.listener(Self::open_action))
            .on_action(cx.listener(Self::download_action))
            .on_action(cx.listener(Self::chmod_action))
            .on_action(cx.listener(Self::toggle_filter_action))
            .on_action(cx.listener(Self::edit_path_action))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.deploy_context_menu(event.position, None, window, cx);
                }),
            )
            .child(self.render_title_bar(window, cx))
            .when(is_connected, |el| {
                el.child(self.render_breadcrumb(window, cx))
            })
            .when(self.filter_editor.is_some(), |el| {
                if let Some(editor) = &self.filter_editor {
                    el.child(
                        h_flex()
                            .px_2()
                            .py_1()
                            .child(editor.clone()),
                    )
                } else {
                    el
                }
            })
            .child(
                v_flex()
                    .flex_grow()
                    .min_h_0()
                    .drag_over::<ExternalPaths>(|style, _, _, cx| {
                        style.bg(cx.theme().colors().drop_target_background)
                    })
                    .on_drop(cx.listener(
                        |this, paths: &ExternalPaths, _window, cx| {
                            this.upload_external_files(paths.paths(), cx);
                        },
                    ))
                    .child(if item_count > 0 {
                        uniform_list(
                            "sftp-file-list",
                            item_count,
                            cx.processor(|this, range, window, cx| {
                                this.render_entries(range, window, cx)
                            }),
                        )
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll_handle)
                        .into_any_element()
                    } else {
                        self.render_empty_state().into_any_element()
                    })
                    .when_some(
                        self.edit_state
                            .as_ref()
                            .filter(|s| s.index.is_none()),
                        |el, edit_state| {
                            let icon = if edit_state.is_dir {
                                IconName::Folder
                            } else {
                                IconName::File
                            };
                            el.child(
                                ListItem::new("sftp-new-entry")
                                    .spacing(ListItemSpacing::Sparse)
                                    .toggle_state(true)
                                    .start_slot(Icon::new(icon).size(IconSize::Small))
                                    .child(edit_state.editor.clone()),
                            )
                        },
                    ),
            )
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(anchored().position(*position).child(menu.clone()))
            }))
    }
}

impl Focusable for SftpPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SftpPanel {
    fn persistent_name() -> &'static str {
        "SFTP Browser"
    }

    fn panel_key() -> &'static str {
        SFTP_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(280.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Folder)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("sftp_panel.title")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        1
    }
}

/// Extracts a CWD path from a prompt string.
/// Matches patterns like `user@host:/var/log$`, `user@host:~$`, `[user@host /tmp]$`,
/// BusyBox-style `/var/log #`, `~ $`, and space-separated `user@host /data$`.
/// Also matches bare directory names from bash `\W` (e.g., `[user@host tmp]$`).
fn extract_cwd_from_prompt(prompt: &str) -> Option<String> {
    static CWD_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = CWD_PATTERNS.get_or_init(|| {
        vec![
            // user@host:/path/to/dir$ or user@host:~/subdir#
            Regex::new(r"[@:]([/~][^\s$#>\]]*)[$#>]\s*$").unwrap(),
            // [user@host /path]$ or [user@host ~]$
            Regex::new(r"\s([/~][^\]$#>]*)[\]$#>]\s*$").unwrap(),
            // user@host /path$ — space-separated (no colon)
            Regex::new(r"@\S+\s+([/~][^\s$#>\]]*)[$#>\]]\s*$").unwrap(),
            // BusyBox: /path/to/dir # or ~ $ — path at start of prompt
            Regex::new(r"^([/~][^\s$#>]*)\s*[$#>]\s*$").unwrap(),
            // [user@host dirname]$ — bare directory name (bash \W)
            Regex::new(r"\s(\w[\w.\-]*)\]\s*[$#>]\s*$").unwrap(),
            // user@host:dirname$ — bare directory name after colon
            Regex::new(r":(\w[\w.\-]*)[$#>]\s*$").unwrap(),
        ]
    });

    for pattern in patterns {
        if let Some(caps) = pattern.captures(prompt) {
            if let Some(path_match) = caps.get(1) {
                let path = path_match.as_str().trim();
                if !path.is_empty() {
                    return Some(path.to_string());
                }
            }
        }
    }
    None
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Sanitize remote directory path to a safe local path component.
/// Strips leading '/', replaces Windows-illegal chars with '_'.
fn sanitize_remote_dir_to_path(remote_dir: &str) -> PathBuf {
    let stripped = remote_dir.trim_start_matches('/');
    if stripped.is_empty() {
        return PathBuf::from("_root");
    }
    let sanitized = stripped.replace(
        |c: char| matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'),
        "_",
    );
    PathBuf::from(sanitized)
}

/// Generate a unique download path by appending (1), (2), etc. if file exists.
fn unique_download_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, ext) = name
        .rsplit_once('.')
        .map(|(s, e)| (s, Some(e)))
        .unwrap_or((name, None));
    for i in 1.. {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, i, e),
            None => format!("{} ({})", stem, i),
        };
        let new_path = dir.join(&new_name);
        if !new_path.exists() {
            return new_path;
        }
    }
    unreachable!()
}

pub fn init(cx: &mut App) {
    SftpStoreEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<SftpPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &RefreshDirectory, _window, cx| {
            if let Some(panel) = workspace.panel::<SftpPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.sync_from_terminal(cx);
                });
            }
        });
        workspace.register_action(|workspace, _: &ToggleWatch, _window, cx| {
            if let Some(panel) = workspace.panel::<SftpPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.toggle_watch(cx);
                });
            }
        });
    })
    .detach();
}
