use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use bspterm_actions::sftp_panel::{RefreshDirectory, ToggleFocus};
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, ClickEvent, Context, Entity,
    EventEmitter, ExternalPaths, FocusHandle, Focusable, IntoElement, ListSizingBehavior,
    ParentElement, Render, Styled, Subscription, Task, UniformListScrollHandle, WeakEntity, Window,
    px, uniform_list,
};
use i18n::t;
use panel::PanelHeader;
use regex::Regex;
use terminal::connection::ssh::{RemoteEntry, SftpClient, SshAuthConfig, SshConfig, SshHostKey};
use terminal::sftp_store::{SftpStore, SftpStoreEntity, SftpStoreEvent};
use terminal::ConnectionInfo;
use ui::{
    prelude::*, Color, Icon, IconButton, IconName, IconSize, Label, LabelSize, ListItem,
    ListItemSpacing, Tooltip, h_flex, v_flex,
};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const SFTP_PANEL_KEY: &str = "SftpPanel";

#[derive(Clone, Debug, PartialEq)]
enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Clone, Debug)]
struct FileEntry {
    entry: RemoteEntry,
    depth: usize,
}

pub struct SftpPanel {
    workspace: WeakEntity<Workspace>,
    sftp_store: Entity<SftpStore>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    width: Option<Pixels>,
    status: ConnectionStatus,
    host_key: Option<SshHostKey>,
    client: Option<Arc<SftpClient>>,
    current_path: String,
    home_path: Option<String>,
    entries: Vec<FileEntry>,
    selected_index: Option<usize>,
    _subscriptions: Vec<Subscription>,
    load_task: Option<Task<()>>,
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

    fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sftp_store = SftpStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let store_subscription =
            cx.subscribe(&sftp_store, |this, _, event, cx| match event {
                SftpStoreEvent::ClientConnected(host_key) => {
                    if this.host_key.as_ref() == Some(host_key) {
                        this.status = ConnectionStatus::Connected;
                        this.client =
                            this.sftp_store.read(cx).get_client(host_key);
                        cx.notify();
                    }
                }
                SftpStoreEvent::ClientDisconnected(host_key) => {
                    if this.host_key.as_ref() == Some(host_key) {
                        this.status = ConnectionStatus::Disconnected;
                        this.client = None;
                        this.entries.clear();
                        cx.notify();
                    }
                }
            });

        Self {
            workspace: workspace.weak_handle(),
            sftp_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            width: None,
            status: ConnectionStatus::Disconnected,
            host_key: None,
            client: None,
            current_path: "/".to_string(),
            home_path: None,
            entries: Vec::new(),
            selected_index: None,
            _subscriptions: vec![store_subscription],
            load_task: None,
        }
    }

    pub fn connect(&mut self, config: SshConfig, cx: &mut Context<Self>) {
        let host_key = SshHostKey::from(&config);
        self.host_key = Some(host_key);
        self.status = ConnectionStatus::Connecting;
        self.entries.clear();
        self.current_path = "/".to_string();
        cx.notify();

        let task = self
            .sftp_store
            .update(cx, |store, cx| store.get_or_connect(config, cx));

        let tokio_handle = gpui_tokio::Tokio::handle(cx);
        cx.spawn(async move |this, cx| {
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
                        if let Ok(home) = &home {
                            this.home_path = Some(home.clone());
                            this.current_path = home.clone();
                        }
                        this.load_directory(cx);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, cx| {
                        this.status = ConnectionStatus::Error(err.to_string());
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub fn connect_from_active_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

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
            self.connect(config, cx);
        }
    }

    fn sync_from_terminal(&mut self, cx: &mut Context<Self>) {
        if self.status != ConnectionStatus::Connected {
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
                // Absolute path — navigate directly
                self.current_path = path;
                self.entries.clear();
                self.selected_index = None;
                self.load_directory(cx);
                cx.notify();
            } else if path.starts_with('~') {
                // Tilde path — replace ~ with cached home_path locally
                if let Some(home) = &self.home_path {
                    let resolved = if path == "~" {
                        home.clone()
                    } else {
                        // ~/subdir → /home/user/subdir
                        format!("{}{}", home, &path[1..])
                    };
                    self.current_path = resolved;
                    self.entries.clear();
                    self.selected_index = None;
                    self.load_directory(cx);
                    cx.notify();
                } else {
                    // No cached home_path, fallback to refresh current
                    self.load_directory(cx);
                }
            } else {
                // Bare directory name (e.g., "tmp", "log") — try multiple candidates
                self.resolve_bare_dirname(path, cx);
            }
        } else {
            self.load_directory(cx);
        }
    }

    fn resolve_bare_dirname(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let current_path = self.current_path.clone();
        let home_path = self.home_path.clone();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn(async move |this, cx| {
            // Build candidate paths to try
            let mut candidates = Vec::new();
            // 1. /{name} (e.g., /tmp)
            candidates.push(format!("/{}", name));
            // 2. {current_path}/{name} (e.g., /home/user/projects)
            let under_current = if current_path.ends_with('/') {
                format!("{}{}", current_path, name)
            } else {
                format!("{}/{}", current_path, name)
            };
            candidates.push(under_current);
            // 3. {home_path}/{name} (e.g., /home/user/dirname)
            if let Some(home) = &home_path {
                let under_home = if home.ends_with('/') {
                    format!("{}{}", home, name)
                } else {
                    format!("{}/{}", home, name)
                };
                candidates.push(under_home);
            }

            // Deduplicate while preserving order
            let mut seen = std::collections::HashSet::new();
            candidates.retain(|c| seen.insert(c.clone()));


            // Try each candidate via stat
            for candidate in &candidates {
                let client_clone = client.clone();
                let path = candidate.clone();
                let result = tokio_handle
                    .spawn(async move { client_clone.stat(&path).await })
                    .await;
                if let Ok(Ok(entry)) = result {
                    if entry.is_dir {
                        this.update(cx, |this, cx| {
                            this.current_path = candidate.clone();
                            this.entries.clear();
                            this.selected_index = None;
                            this.load_directory(cx);
                            cx.notify();
                        }).ok();
                        return;
                    }
                }
            }

            // All candidates failed — fallback to refreshing current directory
            this.update(cx, |this, cx| {
                this.load_directory(cx);
            }).ok();
        }).detach();
    }

    fn load_directory(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let path = self.current_path.clone();
        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = tokio_handle
                .spawn(async move { client.list_dir(&path).await })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r);

            match result {
                Ok(entries) => {
                    this.update(cx, |this, cx| {
                        this.entries = entries
                            .into_iter()
                            .map(|entry| FileEntry { depth: 0, entry })
                            .collect();
                        this.load_task = None;
                        cx.notify();
                    })
                    .ok();
                }
                Err(err) => {
                    log::error!("Failed to list directory: {}", err);
                    this.update(cx, |this, cx| {
                        this.load_task = None;
                        cx.notify();
                    })
                    .ok();
                }
            }
        }));
    }

    fn navigate_up(&mut self, cx: &mut Context<Self>) {
        if self.current_path == "/" {
            return;
        }
        if let Some(parent) = self
            .current_path
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
        {
            self.current_path = parent.to_string();
            self.entries.clear();
            self.selected_index = None;
            self.load_directory(cx);
            cx.notify();
        }
    }

    fn navigate_into(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(index) {
            if entry.entry.is_dir {
                self.current_path = entry.entry.path.clone();
                self.entries.clear();
                self.selected_index = None;
                self.load_directory(cx);
                cx.notify();
            }
        }
    }

    fn upload_external_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let current_path = self.current_path.clone();
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
        self.selected_index = Some(index);
        cx.notify();
    }

    fn on_entry_double_click(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_into(index, cx);
    }

    // Rendering

    fn render_title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status_text = match &self.status {
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
                    .when(self.status == ConnectionStatus::Connected, |this| {
                        this.child(
                            IconButton::new("sftp-refresh", IconName::ArrowCircle)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(t("sftp_panel.refresh")))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.sync_from_terminal(cx);
                                })),
                        )
                    })
                    .child(
                        IconButton::new("sftp-connect", IconName::Link)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(t("sftp_panel.connect_from_terminal")))
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.connect_from_active_terminal(cx);
                            })),
                    ),
            )
    }

    fn render_breadcrumb(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(
                Label::new(self.current_path.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_entries(
        &self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range.map(|ix| self.render_entry(ix, cx)).collect()
    }

    fn render_entry(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(file_entry) = self.entries.get(index) else {
            return gpui::Empty.into_any();
        };

        let entry = &file_entry.entry;
        let is_selected = self.selected_index == Some(index);
        let icon = if entry.is_dir {
            IconName::Folder
        } else {
            IconName::File
        };

        let size_label = if !entry.is_dir {
            format_file_size(entry.size)
        } else {
            String::new()
        };

        ListItem::new(("sftp-entry", index))
            .spacing(ListItemSpacing::Sparse)
            .toggle_state(is_selected)
            .indent_level(file_entry.depth)
            .indent_step_size(px(12.))
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
                    .when(!size_label.is_empty(), |el| {
                        el.child(
                            Label::new(size_label)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let is_double_click = matches!(event, ClickEvent::Mouse(mouse) if mouse.down.click_count == 2);
                if is_double_click {
                    this.on_entry_double_click(index, window, cx);
                } else {
                    this.on_entry_click(index, event, window, cx);
                }
            }))
            .into_any_element()
    }

    fn render_empty_state(&self) -> impl IntoElement {
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
            .child(match &self.status {
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
        let item_count = self.entries.len();
        let is_connected = self.status == ConnectionStatus::Connected;

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .child(self.render_title_bar(window, cx))
            .when(is_connected, |el| el.child(self.render_breadcrumb(cx)))
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
                    }),
            )
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
    })
    .detach();
}
