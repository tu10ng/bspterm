mod group_edit_modal;
mod quick_add;
mod session_edit_modal;

use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

use anyhow::Result;
use editor::Editor;
use editor::actions::{Backtab, Tab};
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, ClickEvent, ClipboardItem,
    Context, DismissEvent, DragMoveEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ListSizingBehavior, MouseButton, MouseDownEvent, ParentElement, Point, Render,
    Styled, Subscription, Task, UniformListScrollHandle, WeakEntity, Window, anchored, deferred,
    px, uniform_list,
};
use i18n::t;
use terminal::{
    AuthMethod, ProtocolConfig, RecognizeConfigEntity, SessionConfig, SessionGroup, SessionNode,
    SessionStoreEntity, SessionStoreEvent, SortMode,
};
use ui::{
    prelude::*, Color, ContextMenu, Disclosure, Icon, IconName, IconPosition, IconSize, Indicator,
    Label, LabelSize, ListItem, ListItemSpacing, PopoverMenu, Tooltip, h_flex, v_flex,
};
use uuid::Uuid;
use workspace::{
    Pane, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};
use panel::{PanelHeader, panel_icon_button};
use bspterm_actions::remote_explorer::{ToggleCollapseAll, ToggleFocus};

use group_edit_modal::GroupEditModal;
pub use quick_add::*;
pub use session_edit_modal::SessionEditModal;

const REMOTE_EXPLORER_PANEL_KEY: &str = "RemoteExplorerPanel";

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum PingStatus {
    #[default]
    Unknown,
    Checking,
    Reachable,
    Unreachable,
}

fn format_session_env_info(session: &SessionConfig) -> String {
    match &session.protocol {
        ProtocolConfig::Telnet(telnet) => {
            let host_with_port = if telnet.port == 23 {
                telnet.host.clone()
            } else {
                format!("{}:{}", telnet.host, telnet.port)
            };
            let username = telnet.username.as_deref().unwrap_or("");
            let password = telnet.password.as_deref().unwrap_or("");
            format!("环境{}\t{}\t{}", host_with_port, username, password)
        }
        ProtocolConfig::Ssh(ssh) => {
            let host_with_port = if ssh.port == 22 {
                ssh.host.clone()
            } else {
                format!("{}:{}", ssh.host, ssh.port)
            };
            let username = ssh.username.as_deref().unwrap_or("");
            let password = match &ssh.auth {
                AuthMethod::Password { password } => password.as_str(),
                _ => "",
            };
            format!("后台{}\t{}\t{}", host_with_port, username, password)
        }
    }
}

fn collect_sessions_from_group(group: &SessionGroup) -> Vec<&SessionConfig> {
    let mut sessions = Vec::new();
    for node in &group.children {
        match node {
            SessionNode::Session(session) => sessions.push(session),
            SessionNode::Group(child_group) => {
                sessions.extend(collect_sessions_from_group(child_group));
            }
        }
    }
    sessions
}

pub fn init(cx: &mut App) {
    SessionStoreEntity::init(cx);
    RecognizeConfigEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<RemoteExplorer>(window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleCollapseAll, window, cx| {
            if let Some(panel) = workspace.panel::<RemoteExplorer>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.toggle_collapse_all(&ToggleCollapseAll, window, cx);
                });
            }
        });
    })
    .detach();
}

/// A flattened tree entry for uniform list rendering.
#[derive(Clone, Debug)]
pub struct FlattenedEntry {
    pub id: Uuid,
    pub depth: usize,
    pub node: SessionNode,
}

/// Data attached to drag operations.
#[derive(Clone)]
struct DraggedSessionEntry {
    id: Uuid,
    name: String,
    is_group: bool,
}

/// Drop target indicator.
#[derive(Clone, PartialEq)]
enum DragTarget {
    IntoGroup { group_id: Uuid },
    BeforeEntry { entry_id: Uuid },
    AfterEntry { entry_id: Uuid },
    Root,
}

/// Visual representation during drag.
struct DraggedSessionView {
    name: String,
    is_group: bool,
}

impl Render for DraggedSessionView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = if self.is_group {
            IconName::Folder
        } else {
            IconName::Server
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

pub struct RemoteExplorer {
    session_store: Entity<SessionStoreEntity>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    visible_entries: Vec<FlattenedEntry>,
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    quick_add_expanded: bool,
    quick_add_area: QuickAddArea,
    selected_entry_id: Option<Uuid>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    drag_target: Option<DragTarget>,
    hover_expand_task: Option<Task<()>>,
    ping_status: HashMap<Uuid, PingStatus>,
    ping_tasks: HashMap<Uuid, Task<()>>,
    ping_refresh_task: Option<Task<()>>,
    filter_editor: Entity<Editor>,
    _filter_subscription: Subscription,
    _subscriptions: Vec<Subscription>,
}

impl RemoteExplorer {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session_store = SessionStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let session_store_subscription =
            cx.subscribe(&session_store, |this, _, event, cx| match event {
                SessionStoreEvent::Changed
                | SessionStoreEvent::SessionAdded(_)
                | SessionStoreEvent::SessionRemoved(_)
                | SessionStoreEvent::CredentialPresetChanged => {
                    this.update_visible_entries(cx);
                }
            });


        let quick_add_area =
            QuickAddArea::new(session_store.clone(), weak_workspace.clone(), window, cx);

        let username_editor = quick_add_area.telnet_section.username_editor.clone();
        let password_editor = quick_add_area.telnet_section.password_editor.clone();

        let username_subscription =
            cx.subscribe(&username_editor, |this, _, event: &editor::EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::BufferEdited { .. }) {
                    if this.quick_add_area.telnet_section.programmatic_change_count > 0 {
                        this.quick_add_area.telnet_section.programmatic_change_count -= 1;
                    } else {
                        this.quick_add_area.telnet_section.clear_credential_selection();
                        cx.notify();
                    }
                }
            });

        let password_subscription =
            cx.subscribe(&password_editor, |this, _, event: &editor::EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::BufferEdited { .. }) {
                    if this.quick_add_area.telnet_section.programmatic_change_count > 0 {
                        this.quick_add_area.telnet_section.programmatic_change_count -= 1;
                    } else {
                        this.quick_add_area.telnet_section.clear_credential_selection();
                        cx.notify();
                    }
                }
            });

        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("remote_explorer.filter_placeholder"), window, cx);
            editor
        });

        let filter_subscription =
            cx.subscribe(&filter_editor, |this, _, event: &editor::EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::BufferEdited { .. }) {
                    this.update_visible_entries(cx);
                    cx.notify();
                }
            });

        let subscriptions = vec![
            session_store_subscription,
            username_subscription,
            password_subscription,
        ];

        let mut this = Self {
            session_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            visible_entries: Vec::new(),
            workspace: weak_workspace,
            width: None,
            quick_add_expanded: true,
            quick_add_area,
            selected_entry_id: None,
            context_menu: None,
            drag_target: None,
            hover_expand_task: None,
            ping_status: HashMap::new(),
            ping_tasks: HashMap::new(),
            ping_refresh_task: None,
            filter_editor,
            _filter_subscription: filter_subscription,
            _subscriptions: subscriptions,
        };

        this.update_visible_entries(cx);
        this.start_ping_refresh_loop(window, cx);
        this
    }

    fn update_visible_entries(&mut self, cx: &mut Context<Self>) {
        let session_store = self.session_store.read(cx);
        let store = session_store.store();
        let sort_mode = store.sort_mode;

        let sorted_root = Self::sort_nodes(&store.root, sort_mode);

        let filter_query = self.filter_editor.read(cx).text(cx);
        let filter_query = filter_query.trim().to_lowercase();

        let filtered_root = if filter_query.is_empty() {
            sorted_root
        } else {
            Self::filter_nodes(&sorted_root, &filter_query)
        };

        let mut entries = Vec::new();
        if filter_query.is_empty() {
            Self::flatten_nodes_sorted(&filtered_root, 0, sort_mode, &mut entries);
        } else {
            Self::flatten_nodes_expanded(&filtered_root, 0, sort_mode, &mut entries);
        }
        self.visible_entries = entries;
        self.schedule_ping_for_visible_sessions(cx);
    }

    fn filter_nodes(nodes: &[SessionNode], query: &str) -> Vec<SessionNode> {
        nodes
            .iter()
            .filter_map(|node| match node {
                SessionNode::Group(group) => {
                    if group.name.to_lowercase().contains(query) {
                        return Some(node.clone());
                    }
                    if Self::group_has_matching_session(group, query) {
                        return Some(node.clone());
                    }
                    let filtered_children = Self::filter_nodes(&group.children, query);
                    if !filtered_children.is_empty() {
                        let mut filtered_group = group.clone();
                        filtered_group.children = filtered_children;
                        return Some(SessionNode::Group(filtered_group));
                    }
                    None
                }
                SessionNode::Session(session) => {
                    if Self::session_matches(session, query) {
                        Some(node.clone())
                    } else {
                        None
                    }
                }
            })
            .collect()
    }

    fn session_matches(session: &SessionConfig, query: &str) -> bool {
        if session.name.to_lowercase().contains(query) {
            return true;
        }
        if session.tags.iter().any(|tag| tag.to_lowercase().contains(query)) {
            return true;
        }
        match &session.protocol {
            ProtocolConfig::Ssh(ssh) => {
                ssh.host.to_lowercase().contains(query)
                    || ssh
                        .username
                        .as_deref()
                        .is_some_and(|u| u.to_lowercase().contains(query))
            }
            ProtocolConfig::Telnet(telnet) => {
                telnet.host.to_lowercase().contains(query)
                    || telnet
                        .username
                        .as_deref()
                        .is_some_and(|u| u.to_lowercase().contains(query))
            }
        }
    }

    fn group_has_matching_session(group: &SessionGroup, query: &str) -> bool {
        group.children.iter().any(|child| match child {
            SessionNode::Session(session) => Self::session_matches(session, query),
            SessionNode::Group(child_group) => {
                Self::group_has_matching_session(child_group, query)
            }
        })
    }

    fn flatten_nodes_expanded(
        nodes: &[SessionNode],
        depth: usize,
        sort_mode: SortMode,
        result: &mut Vec<FlattenedEntry>,
    ) {
        for node in nodes {
            result.push(FlattenedEntry {
                id: node.id(),
                depth,
                node: node.clone(),
            });

            if let SessionNode::Group(group) = node {
                let sorted_children = Self::sort_nodes(&group.children, sort_mode);
                Self::flatten_nodes_expanded(&sorted_children, depth + 1, sort_mode, result);
            }
        }
    }

    fn has_any_expanded_group(&self, cx: &App) -> bool {
        fn check_recursive(nodes: &[SessionNode]) -> bool {
            nodes.iter().any(|node| {
                if let SessionNode::Group(group) = node {
                    group.expanded || check_recursive(&group.children)
                } else {
                    false
                }
            })
        }
        let store = self.session_store.read(cx);
        check_recursive(&store.store().root)
    }

    fn toggle_collapse_all(
        &mut self,
        _: &ToggleCollapseAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_expanded = self.has_any_expanded_group(cx);
        if has_expanded {
            self.session_store.update(cx, |store, cx| {
                store.collapse_all_groups(cx);
            });
        } else {
            self.session_store.update(cx, |store, cx| {
                store.expand_all_groups(cx);
            });
        }
        self.update_visible_entries(cx);
    }

    fn render_title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel_header_container(window, cx)
            .px_2()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .gap_1()
                    .child(Icon::new(IconName::Server).color(Color::Muted).size(IconSize::Small))
                    .child(Label::new(t("remote_explorer.title")).size(LabelSize::Small)),
            )
    }

    fn render_remote_list_header(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let has_expanded = self.has_any_expanded_group(cx);
        let has_query = !self.filter_editor.read(cx).text(cx).is_empty();
        let tooltip_text = if has_expanded {
            t("remote_explorer.collapse_all")
        } else {
            t("remote_explorer.expand_all")
        };

        v_flex()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .px_2()
                    .py_1()
                    .child(
                        div().flex_1().child(
                            Label::new(t("remote_explorer.sessions"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_0p5()
                            .child(self.render_sort_button(cx))
                            .child({
                                let icon = if has_expanded {
                                    IconName::CollapseAll
                                } else {
                                    IconName::ExpandAll
                                };
                                panel_icon_button("toggle-collapse", icon)
                                    .icon_size(IconSize::Small)
                                    .tooltip(move |_window, cx| {
                                        Tooltip::for_action_in(
                                            tooltip_text.clone(),
                                            &ToggleCollapseAll,
                                            &focus_handle,
                                            cx,
                                        )
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_collapse_all(
                                            &ToggleCollapseAll,
                                            window,
                                            cx,
                                        );
                                    }))
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_16()
                            .overflow_hidden()
                            .child(self.filter_editor.clone()),
                    )
                    .when(has_query, |this| {
                        this.child(
                            panel_icon_button("clear-filter", IconName::Close)
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.filter_editor.update(cx, |editor, cx| {
                                        editor.set_text("", window, cx);
                                    });
                                })),
                        )
                    }),
            )
    }

    fn render_sort_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session_store = self.session_store.clone();
        let current_sort_mode = self.session_store.read(cx).sort_mode();

        PopoverMenu::new("sort-popover")
            .trigger(
                panel_icon_button("sort-button", IconName::ListFilter)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text(t("remote_explorer.sort_by"))),
            )
            .menu(move |window, cx| {
                let session_store = session_store.clone();
                Some(ContextMenu::build(window, cx, move |menu, _window, _cx| {
                    let session_store_for_name = session_store.clone();
                    menu.toggleable_entry(
                        t("remote_explorer.sort_addition_order"),
                        current_sort_mode == SortMode::AdditionOrder,
                        IconPosition::Start,
                        None,
                        {
                            let session_store = session_store.clone();
                            move |_window, cx| {
                                session_store.update(cx, |store, cx| {
                                    store.set_sort_mode(SortMode::AdditionOrder, cx);
                                });
                            }
                        },
                    )
                    .toggleable_entry(
                        t("remote_explorer.sort_name_ascending"),
                        current_sort_mode == SortMode::NameAscending,
                        IconPosition::Start,
                        None,
                        move |_window, cx| {
                            session_store_for_name.update(cx, |store, cx| {
                                store.set_sort_mode(SortMode::NameAscending, cx);
                            });
                        },
                    )
                }))
            })
            .anchor(gpui::Corner::TopRight)
    }

    fn sort_nodes(nodes: &[SessionNode], sort_mode: SortMode) -> Vec<SessionNode> {
        let mut pinned_groups: Vec<SessionNode> = Vec::new();
        let mut unpinned_groups: Vec<SessionNode> = Vec::new();
        let mut sessions: Vec<SessionNode> = Vec::new();

        for node in nodes {
            match node {
                SessionNode::Group(group) => {
                    if group.pinned {
                        pinned_groups.push(node.clone());
                    } else {
                        unpinned_groups.push(node.clone());
                    }
                }
                SessionNode::Session(_) => {
                    sessions.push(node.clone());
                }
            }
        }

        if sort_mode == SortMode::NameAscending {
            pinned_groups.sort_by(|a, b| {
                natural_sort_key(&a.name().to_lowercase())
                    .cmp(&natural_sort_key(&b.name().to_lowercase()))
            });
            unpinned_groups.sort_by(|a, b| {
                natural_sort_key(&a.name().to_lowercase())
                    .cmp(&natural_sort_key(&b.name().to_lowercase()))
            });
        }

        let mut result = Vec::with_capacity(nodes.len());
        result.extend(pinned_groups);
        result.extend(unpinned_groups);
        result.extend(sessions);
        result
    }

    fn flatten_nodes_sorted(
        nodes: &[SessionNode],
        depth: usize,
        sort_mode: SortMode,
        result: &mut Vec<FlattenedEntry>,
    ) {
        for node in nodes {
            result.push(FlattenedEntry {
                id: node.id(),
                depth,
                node: node.clone(),
            });

            if let SessionNode::Group(group) = node {
                if group.expanded {
                    let sorted_children = Self::sort_nodes(&group.children, sort_mode);
                    Self::flatten_nodes_sorted(&sorted_children, depth + 1, sort_mode, result);
                }
            }
        }
    }

    fn start_ping_refresh_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_ping_for_visible_sessions(cx);

        self.ping_refresh_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(5))
                    .await;
                this.update(&mut cx.clone(), |this, cx| {
                    this.schedule_ping_for_visible_sessions(cx);
                })
                .ok();
            }
        }));
    }

    fn schedule_ping_for_visible_sessions(&mut self, cx: &mut Context<Self>) {
        for entry in &self.visible_entries {
            if let SessionNode::Session(session) = &entry.node {
                let id = entry.id;

                if self.ping_tasks.contains_key(&id) {
                    continue;
                }

                let (host, port) = match &session.protocol {
                    ProtocolConfig::Ssh(ssh) => (ssh.host.clone(), ssh.port),
                    ProtocolConfig::Telnet(telnet) => (telnet.host.clone(), telnet.port),
                };

                // Only show Checking status on first check, preserve existing status on re-check
                // This prevents flickering between Checking (gray) and Unreachable (red)
                self.ping_status.entry(id).or_insert(PingStatus::Checking);

                let task = cx.spawn(async move |this, cx| {
                    let executor = cx.background_executor().clone();
                    let reachable = executor
                        .spawn(async move {
                            util::reachability::check_reachability(&host, port).await
                        })
                        .await;

                    this.update(&mut cx.clone(), |this, cx| {
                        let status = if reachable {
                            PingStatus::Reachable
                        } else {
                            PingStatus::Unreachable
                        };
                        this.ping_status.insert(id, status);
                        this.ping_tasks.remove(&id);
                        cx.notify();
                    })
                    .ok();
                });

                self.ping_tasks.insert(id, task);
            }
        }
        cx.notify();
    }

    fn toggle_expanded(&mut self, id: Uuid, _window: &mut Window, cx: &mut Context<Self>) {
        self.session_store.update(cx, |store, cx| {
            store.toggle_group_expanded(id, cx);
        });
        self.update_visible_entries(cx);
    }

    fn toggle_quick_add(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.quick_add_expanded = !self.quick_add_expanded;
        cx.notify();
    }

    fn select_entry(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.selected_entry_id = Some(id);
        cx.notify();
    }

    fn connect_session(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let session_store = self.session_store.read(cx);
        let Some(node) = session_store.store().find_node(id) else {
            return;
        };

        let SessionNode::Session(session) = node else {
            return;
        };

        let session_id = session.id;
        match &session.protocol {
            ProtocolConfig::Ssh(ssh_config) => {
                let workspace = self.workspace.clone();
                let pane = self.get_terminal_pane(cx);
                if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
                    connect_ssh(ssh_config.clone(), Some(session_id), workspace, pane, window, cx);
                }
            }
            ProtocolConfig::Telnet(telnet_config) => {
                let workspace = self.workspace.clone();
                let pane = self.get_terminal_pane(cx);
                if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
                    connect_telnet(telnet_config.clone(), Some(session_id), workspace, pane, window, cx);
                }
            }
        }
    }

    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        entry_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_store = self.session_store.read(cx);
        let Some(node) = session_store.store().find_node(entry_id) else {
            return;
        };

        let workspace = self.workspace.clone();
        let session_store_entity = self.session_store.clone();

        let context_menu = match node {
            SessionNode::Session(session) => {
                let is_ssh = matches!(session.protocol, ProtocolConfig::Ssh(_));
                let switch_label = if is_ssh {
                    t("remote_explorer.switch_to_telnet")
                } else {
                    t("remote_explorer.switch_to_ssh")
                };
                let session_store_for_copy = session_store_entity.clone();
                let session_store_for_switch = session_store_entity.clone();
                ContextMenu::build(window, cx, move |menu, _window, _cx| {
                    let workspace_for_edit = workspace.clone();

                    menu.entry(t("remote_explorer.edit_session"), None, move |window, cx| {
                        if let Some(workspace) = workspace_for_edit.upgrade() {
                            workspace.update(cx, |ws, cx| {
                                ws.toggle_modal(window, cx, |window, cx| {
                                    SessionEditModal::new(entry_id, window, cx)
                                });
                            });
                        }
                    })
                    .entry(t("remote_explorer.copy_env_info"), None, {
                        let session_store = session_store_for_copy.clone();
                        move |_window, cx| {
                            let store = session_store.read(cx);
                            if let Some(SessionNode::Session(session)) =
                                store.store().find_node(entry_id)
                            {
                                let info = format_session_env_info(session);
                                cx.write_to_clipboard(ClipboardItem::new_string(info));
                            }
                        }
                    })
                    .entry(switch_label, None, move |_window, cx| {
                        session_store_for_switch.update(cx, |store, cx| {
                            store.update_session(entry_id, |session| {
                                session.switch_protocol();
                            }, cx);
                        });
                    })
                    .entry(t("remote_explorer.delete_session"), None, move |_window, cx| {
                        session_store_entity.update(cx, |store, cx| {
                            store.remove_node(entry_id, cx);
                        });
                    })
                })
            }
            SessionNode::Group(group) => {
                let is_pinned = group.pinned;
                let session_store_for_pin = session_store_entity.clone();
                let session_store_for_copy = session_store_entity.clone();
                ContextMenu::build(window, cx, move |menu, _window, _cx| {
                    let workspace_for_edit = workspace.clone();
                    let workspace_for_new_ssh = workspace_for_edit.clone();
                    let workspace_for_new_telnet = workspace_for_edit.clone();
                    let workspace_for_new_subgroup = workspace_for_edit.clone();
                    let pin_label = if is_pinned {
                        t("remote_explorer.unpin_group")
                    } else {
                        t("remote_explorer.pin_group")
                    };

                    menu.entry(pin_label, None, move |_window, cx| {
                        session_store_for_pin.update(cx, |store, cx| {
                            store.toggle_pin_group(entry_id, cx);
                        });
                    })
                    .entry(t("remote_explorer.new_ssh_session"), None, move |window, cx| {
                        if let Some(workspace) = workspace_for_new_ssh.upgrade() {
                            workspace.update(cx, |ws, cx| {
                                ws.toggle_modal(window, cx, |window, cx| {
                                    SessionEditModal::new_create_ssh(Some(entry_id), window, cx)
                                });
                            });
                        }
                    })
                    .entry(t("remote_explorer.new_telnet_session"), None, move |window, cx| {
                        if let Some(workspace) = workspace_for_new_telnet.upgrade() {
                            workspace.update(cx, |ws, cx| {
                                ws.toggle_modal(window, cx, |window, cx| {
                                    SessionEditModal::new_create_telnet(Some(entry_id), window, cx)
                                });
                            });
                        }
                    })
                    .entry(t("remote_explorer.new_subgroup"), None, move |window, cx| {
                        if let Some(workspace) = workspace_for_new_subgroup.upgrade() {
                            workspace.update(cx, |ws, cx| {
                                ws.toggle_modal(window, cx, |window, cx| {
                                    GroupEditModal::new_create(Some(entry_id), window, cx)
                                });
                            });
                        }
                    })
                    .separator()
                    .entry(t("remote_explorer.rename_group"), None, move |window, cx| {
                        if let Some(workspace) = workspace_for_edit.upgrade() {
                            workspace.update(cx, |ws, cx| {
                                ws.toggle_modal(window, cx, |window, cx| {
                                    GroupEditModal::new_edit(entry_id, window, cx)
                                });
                            });
                        }
                    })
                    .entry(t("remote_explorer.copy_env_info"), None, {
                        let session_store = session_store_for_copy.clone();
                        move |_window, cx| {
                            let store = session_store.read(cx);
                            if let Some(SessionNode::Group(group)) =
                                store.store().find_node(entry_id)
                            {
                                let sessions = collect_sessions_from_group(group);
                                let info = sessions
                                    .iter()
                                    .map(|s| format_session_env_info(s))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                cx.write_to_clipboard(ClipboardItem::new_string(info));
                            }
                        }
                    })
                    .entry(t("remote_explorer.delete_group"), None, move |_window, cx| {
                        session_store_entity.update(cx, |store, cx| {
                            store.remove_node(entry_id, cx);
                        });
                    })
                })
            }
        };

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn deploy_blank_area_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        let has_expanded = self.has_any_expanded_group(cx);
        let toggle_label = if has_expanded {
            t("remote_explorer.collapse_all")
        } else {
            t("remote_explorer.expand_all")
        };

        let context_menu = ContextMenu::build(window, cx, move |menu, _window, _cx| {
            menu.action(toggle_label, ToggleCollapseAll.boxed_clone())
                .separator()
                .entry(t("remote_explorer.new_group"), None, move |window, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update(cx, |ws, cx| {
                            ws.toggle_modal(window, cx, |window, cx| {
                                GroupEditModal::new_create(None, window, cx)
                            });
                        });
                    }
                })
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn get_terminal_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        let workspace = self.workspace.upgrade()?;
        let workspace = workspace.read(cx);
        Some(workspace.active_pane().clone())
    }

    fn handle_auto_recognize_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let pane = self.get_terminal_pane(cx);
        if let Some(result) = self
            .quick_add_area
            .handle_auto_recognize_confirm(workspace, pane, window, cx)
        {
            match result {
                ConnectionResult::Ssh(ssh_config, session_id, workspace, pane) => {
                    connect_ssh(ssh_config, Some(session_id), workspace, pane, window, cx);
                }
                ConnectionResult::Telnet(telnet_config, session_id, workspace, pane) => {
                    connect_telnet(telnet_config, Some(session_id), workspace, pane, window, cx);
                }
            }
        }
    }

    fn handle_telnet_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let pane = self.get_terminal_pane(cx);
        if let Some((telnet_config, session_id, workspace, pane)) = self
            .quick_add_area
            .handle_telnet_connect(workspace, pane, window, cx)
        {
            connect_telnet(telnet_config, Some(session_id), workspace, pane, window, cx);
        }
    }

    fn handle_ssh_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let pane = self.get_terminal_pane(cx);
        if let Some((ssh_config, session_id, workspace, pane)) = self
            .quick_add_area
            .handle_ssh_connect(workspace, pane, window, cx)
        {
            connect_ssh(ssh_config, Some(session_id), workspace, pane, window, cx);
        }
    }

    fn render_quick_add_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let expanded = self.quick_add_expanded;

        h_flex()
            .id("quick-add-header")
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .cursor_pointer()
            .hover(|style| style.bg(theme.colors().ghost_element_hover))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.toggle_quick_add(window, cx);
            }))
            .child(Disclosure::new("quick-add-disclosure", expanded))
            .child(
                Label::new(t("remote_explorer.quick_add"))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn quick_add_editors(&self) -> Vec<Entity<Editor>> {
        let mut editors = Vec::new();
        editors.push(self.quick_add_area.auto_recognize.editor().clone());
        editors.push(self.quick_add_area.telnet_section.ip_editor.clone());
        editors.push(self.quick_add_area.telnet_section.port_editor.clone());
        editors.push(self.quick_add_area.telnet_section.username_editor.clone());
        editors.push(self.quick_add_area.telnet_section.password_editor.clone());
        editors.push(self.quick_add_area.ssh_section.editor().clone());
        editors.push(self.quick_add_area.ssh_section.username_editor().clone());
        editors.push(self.quick_add_area.ssh_section.password_editor().clone());
        editors
    }

    fn focus_next_quick_add_editor(
        &mut self,
        _: &Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editors = self.quick_add_editors();
        if editors.len() <= 1 {
            return;
        }
        if let Some(current) = editors
            .iter()
            .position(|e| e.focus_handle(cx).is_focused(window))
        {
            let next = (current + 1) % editors.len();
            window.focus(&editors[next].focus_handle(cx), cx);
            cx.stop_propagation();
        }
    }

    fn focus_prev_quick_add_editor(
        &mut self,
        _: &Backtab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editors = self.quick_add_editors();
        if editors.len() <= 1 {
            return;
        }
        if let Some(current) = editors
            .iter()
            .position(|e| e.focus_handle(cx).is_focused(window))
        {
            let prev = if current == 0 {
                editors.len() - 1
            } else {
                current - 1
            };
            window.focus(&editors[prev].focus_handle(cx), cx);
            cx.stop_propagation();
        }
    }

    fn render_quick_add_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .px_2()
            .pb_2()
            .gap_3()
            .capture_action(cx.listener(Self::focus_next_quick_add_editor))
            .capture_action(cx.listener(Self::focus_prev_quick_add_editor))
            .child(self.render_auto_recognize_section(window, cx))
            .child(self.render_telnet_section(window, cx))
            .child(self.render_ssh_section(window, cx))
    }

    fn render_auto_recognize_section(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let editor = self.quick_add_area.auto_recognize.editor().clone();

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(t("remote_explorer.auto_recognize"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(theme.colors().border)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                this.handle_auto_recognize_confirm(window, cx);
                            }))
                            .child(editor),
                    ),
            )
            .child(
                Label::new(t("remote_explorer.auto_recognize_hint"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }

    fn render_telnet_section(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let session_store = self.session_store.read(cx);
        let credentials = session_store.store().collect_telnet_credentials();
        let credential_label = self.quick_add_area.telnet_section.get_credential_label();

        let ip_editor = self.quick_add_area.telnet_section.ip_editor.clone();
        let port_editor = self.quick_add_area.telnet_section.port_editor.clone();
        let username_editor = self.quick_add_area.telnet_section.username_editor.clone();
        let password_editor = self.quick_add_area.telnet_section.password_editor.clone();

        let this = cx.entity().downgrade();
        let credential_menu = ui::ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
            let this_for_custom = this.clone();
            menu = menu.entry("Custom", None, move |window, cx| {
                if let Some(this) = this_for_custom.upgrade() {
                    this.update(cx, |this, cx| {
                        this.quick_add_area
                            .telnet_section
                            .select_credential(None, window, cx);
                        cx.notify();
                    });
                }
            });
            for (username, password) in &credentials {
                let label = format!("{}/{}", username, password);
                let credential = (username.clone(), password.clone());
                let this_for_cred = this.clone();
                menu = menu.entry(label, None, move |window, cx| {
                    if let Some(this) = this_for_cred.upgrade() {
                        let cred = credential.clone();
                        this.update(cx, |this, cx| {
                            this.quick_add_area
                                .telnet_section
                                .select_credential(Some(cred), window, cx);
                            cx.notify();
                        });
                    }
                });
            }
            menu
        });

        let theme = cx.theme();
        let border_color = theme.colors().border;

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Terminal)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(t("remote_explorer.telnet_quick_connect"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(ip_editor),
                    )
                    .child(
                        div()
                            .w_16()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(port_editor),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        Label::new(t("remote_explorer.credential"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        ui::DropdownMenu::new("telnet-credential", credential_label, credential_menu)
                            .trigger_size(ui::ButtonSize::Compact),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(username_editor),
                    )
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(password_editor),
                    )
                    .child(
                        ui::Button::new("telnet-connect", t("common.connect"))
                            .style(ui::ButtonStyle::Filled)
                            .size(ui::ButtonSize::Compact)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.handle_telnet_connect(window, cx);
                            })),
                    ),
            )
    }

    fn render_ssh_section(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let host_editor = self.quick_add_area.ssh_section.editor().clone();
        let username_editor = self.quick_add_area.ssh_section.username_editor().clone();
        let password_editor = self.quick_add_area.ssh_section.password_editor().clone();

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Server)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(t("remote_explorer.ssh_quick_connect"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                this.handle_ssh_connect(window, cx);
                            }))
                            .child(host_editor),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(username_editor),
                    )
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(password_editor),
                    )
                    .child(
                        ui::Button::new("ssh-connect", t("common.connect"))
                            .style(ui::ButtonStyle::Filled)
                            .size(ui::ButtonSize::Compact)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.handle_ssh_connect(window, cx);
                            })),
                    ),
            )
    }

    fn handle_drag_move(
        &mut self,
        target_id: Uuid,
        target_is_group: bool,
        target_is_expanded: bool,
        dragged_id: Uuid,
        dragged_is_group: bool,
        mouse_y: f32,
        item_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Bounds check: only process if mouse is within this item's bounds.
        // This is necessary because on_drag_move fires for ALL registered handlers
        // during the Capture phase, regardless of hitbox.
        if mouse_y < 0.0 || mouse_y > item_height {
            return;
        }

        if dragged_id == target_id {
            self.drag_target = None;
            self.hover_expand_task = None;
            cx.notify();
            return;
        }

        if dragged_is_group {
            let session_store = self.session_store.read(cx);
            if session_store.store().is_ancestor_of(dragged_id, target_id) {
                self.drag_target = None;
                self.hover_expand_task = None;
                cx.notify();
                return;
            }
        }

        let relative_y = mouse_y / item_height;

        let new_target = if target_is_group {
            if relative_y < 0.25 {
                DragTarget::BeforeEntry { entry_id: target_id }
            } else if relative_y > 0.75 {
                DragTarget::AfterEntry { entry_id: target_id }
            } else {
                DragTarget::IntoGroup { group_id: target_id }
            }
        } else if relative_y < 0.5 {
            DragTarget::BeforeEntry { entry_id: target_id }
        } else {
            DragTarget::AfterEntry { entry_id: target_id }
        };

        let target_changed = self.drag_target.as_ref() != Some(&new_target);
        self.drag_target = Some(new_target.clone());

        if target_changed {
            self.hover_expand_task = None;

            if let DragTarget::IntoGroup { group_id } = &new_target {
                if target_is_group && !target_is_expanded {
                    let group_id = *group_id;
                    let session_store = self.session_store.clone();
                    self.hover_expand_task = Some(cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor().timer(Duration::from_millis(500)).await;
                        this.update(&mut cx.clone(), |this, cx| {
                            session_store.update(cx, |store, cx| {
                                store.expand_group(group_id, cx);
                            });
                            this.update_visible_entries(cx);
                        }).ok();
                    }));
                }
            }

            cx.notify();
        }
    }

    fn handle_drop(
        &mut self,
        dragged: &DraggedSessionEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.drag_target.take() else {
            return;
        };
        self.hover_expand_task = None;

        let session_store = self.session_store.read(cx);
        let store = session_store.store();

        let (new_parent_id, index) = match target {
            DragTarget::IntoGroup { group_id } => {
                let child_count = store
                    .find_node(group_id)
                    .and_then(|n| match n {
                        SessionNode::Group(g) => Some(g.children.len()),
                        _ => None,
                    })
                    .unwrap_or(0);
                (Some(group_id), child_count)
            }
            DragTarget::BeforeEntry { entry_id } => {
                if let Some((parent_id, idx)) = store.find_node_location(entry_id) {
                    (parent_id, idx)
                } else {
                    cx.notify();
                    return;
                }
            }
            DragTarget::AfterEntry { entry_id } => {
                if let Some((parent_id, idx)) = store.find_node_location(entry_id) {
                    (parent_id, idx + 1)
                } else {
                    cx.notify();
                    return;
                }
            }
            DragTarget::Root => (None, store.root.len()),
        };

        let _ = session_store;

        self.session_store.update(cx, |store, cx| {
            store.move_node(dragged.id, new_parent_id, index, cx);
        });

        self.update_visible_entries(cx);
    }

    fn render_entry(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let entry = &self.visible_entries[index];
        let id = entry.id;
        let depth = entry.depth;
        let is_selected = self.selected_entry_id == Some(id);

        let (icon, name, is_group, is_expanded, is_pinned) = match &entry.node {
            SessionNode::Group(group) => (
                if group.expanded {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                },
                group.name.clone(),
                true,
                Some(group.expanded),
                group.pinned,
            ),
            SessionNode::Session(session) => {
                let icon = match &session.protocol {
                    ProtocolConfig::Ssh(_) => IconName::LetterS,
                    ProtocolConfig::Telnet(_) => IconName::LetterT,
                };
                (icon, session.name.clone(), false, None, false)
            }
        };

        let is_expanded_bool = is_expanded.unwrap_or(false);

        let show_before_indicator = matches!(
            &self.drag_target,
            Some(DragTarget::BeforeEntry { entry_id }) if *entry_id == id
        );
        let show_after_indicator = matches!(
            &self.drag_target,
            Some(DragTarget::AfterEntry { entry_id }) if *entry_id == id
        );
        let show_into_highlight = matches!(
            &self.drag_target,
            Some(DragTarget::IntoGroup { group_id }) if *group_id == id
        );

        let theme = cx.theme();
        let accent_color = theme.colors().text_accent;
        let drop_bg = theme.colors().drop_target_background;
        let drop_border = theme.colors().drop_target_border;

        let drag_data = DraggedSessionEntry {
            id,
            name: name.clone(),
            is_group,
        };

        let list_item = ListItem::new(id)
            .indent_level(depth)
            .indent_step_size(px(12.))
            .spacing(ListItemSpacing::Dense)
            .toggle(is_expanded)
            .toggle_state(is_selected)
            .when(is_group, |this| {
                this.on_toggle(cx.listener(move |this, _, window, cx| {
                    this.toggle_expanded(id, window, cx);
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.toggle_expanded(id, window, cx);
                }))
                .on_secondary_mouse_down(cx.listener(
                    move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.select_entry(id, cx);
                        this.deploy_context_menu(event.position, id, window, cx);
                    },
                ))
            })
            .when(!is_group, |this| {
                this.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.connect_session(id, window, cx);
                    } else {
                        this.select_entry(id, cx);
                    }
                }))
                .on_secondary_mouse_down(cx.listener(
                    move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.select_entry(id, cx);
                        this.deploy_context_menu(event.position, id, window, cx);
                    },
                ))
            })
            .start_slot({
                let ping_status = self.ping_status.get(&id).copied().unwrap_or_default();
                let (indicator_color, tooltip_text) = match ping_status {
                    PingStatus::Unknown => (Color::Muted, t("remote_explorer.ping_unknown")),
                    PingStatus::Checking => (Color::Muted, t("remote_explorer.ping_checking")),
                    PingStatus::Reachable => (Color::Success, t("remote_explorer.ping_reachable")),
                    PingStatus::Unreachable => (Color::Error, t("remote_explorer.ping_unreachable")),
                };
                h_flex()
                    .gap_1()
                    .when(!is_group, |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!("ping-indicator-{}", id)))
                                .child(Indicator::dot().color(indicator_color))
                                .tooltip(Tooltip::text(tooltip_text)),
                        )
                    })
                    .child(Icon::new(icon).color(Color::Muted).size(IconSize::Small))
            })
            .child(
                h_flex()
                    .gap_1()
                    .child(Label::new(name))
                    .when(is_pinned, |this| {
                        this.child(
                            Icon::new(IconName::Pin)
                                .color(Color::Muted)
                                .size(IconSize::XSmall),
                        )
                    }),
            )
;

        let before_line = div()
            .w_full()
            .h(px(2.))
            .when(show_before_indicator, |this| this.bg(accent_color));

        let after_line = div()
            .w_full()
            .h(px(2.))
            .when(show_after_indicator, |this| this.bg(accent_color));

        let entry_wrapper = div()
            .id(SharedString::from(format!("entry-wrapper-{}", id)))
            .w_full()
            .when(show_into_highlight, |this| {
                this.bg(drop_bg).border_l_2().border_color(drop_border)
            })
            .on_drag(drag_data, move |drag_data, _click_offset, _window, cx| {
                cx.new(|_| DraggedSessionView {
                    name: drag_data.name.clone(),
                    is_group: drag_data.is_group,
                })
            })
            .on_drag_move::<DraggedSessionEntry>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSessionEntry>, window, cx| {
                    let bounds = event.bounds;
                    let mouse_y = event.event.position.y - bounds.origin.y;
                    let item_height = bounds.size.height;
                    let drag_state = event.drag(cx);
                    this.handle_drag_move(
                        id,
                        is_group,
                        is_expanded_bool,
                        drag_state.id,
                        drag_state.is_group,
                        mouse_y.into(),
                        item_height.into(),
                        window,
                        cx,
                    );
                },
            ))
            .on_drop(cx.listener(
                move |this, dragged: &DraggedSessionEntry, window, cx| {
                    this.handle_drop(dragged, window, cx);
                },
            ))
            .child(list_item);

        v_flex()
            .w_full()
            .child(before_line)
            .child(entry_wrapper)
            .child(after_line)
            .into_any_element()
    }

    fn render_entries(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut items = Vec::with_capacity(range.len());
        for ix in range {
            items.push(self.render_entry(ix, window, cx));
        }
        items
    }
}

impl EventEmitter<PanelEvent> for RemoteExplorer {}

impl PanelHeader for RemoteExplorer {}

impl Render for RemoteExplorer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Clean up drag state when there's no active drag.
        // GPUI clears active_drag when mouse is released, but our drag_target persists.
        if !cx.has_active_drag() && self.drag_target.is_some() {
            self.drag_target = None;
            self.hover_expand_task = None;
        }

        let theme = cx.theme();
        let border_variant = theme.colors().border_variant;
        let accent_color = theme.colors().text_accent;
        let drop_bg = theme.colors().drop_target_background;

        let item_count = self.visible_entries.len();
        let quick_add_expanded = self.quick_add_expanded;
        let show_root_indicator = matches!(self.drag_target, Some(DragTarget::Root));

        v_flex()
            .id("remote-explorer")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::toggle_collapse_all))
            .child(self.render_title_bar(window, cx))
            .child(
                v_flex()
                    .w_full()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(border_variant)
                    .child(self.render_quick_add_header(cx))
                    .when(quick_add_expanded, |this| {
                        this.child(self.render_quick_add_content(window, cx))
                    }),
            )
            .child(self.render_remote_list_header(window, cx))
            .child(
                v_flex()
                    .flex_grow()
                    .min_h_0()
                    .id("remote-explorer-list-container")
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.deploy_blank_area_context_menu(event.position, window, cx);
                        }),
                    )
                    .on_drag_move::<DraggedSessionEntry>(cx.listener(
                        |this, event: &DragMoveEvent<DraggedSessionEntry>, _window, cx| {
                            if event.bounds.contains(&event.event.position) {
                                this.drag_target = Some(DragTarget::Root);
                                this.hover_expand_task = None;
                                cx.notify();
                            }
                        },
                    ))
                    .on_drop(cx.listener(
                        |this, dragged: &DraggedSessionEntry, window, cx| {
                            this.handle_drop(dragged, window, cx);
                        },
                    ))
                    .when(show_root_indicator, |this| {
                        this.bg(drop_bg).border_t_2().border_color(accent_color)
                    })
                    .child(if item_count > 0 {
                        uniform_list(
                            "remote-explorer-list",
                            item_count,
                            cx.processor(|this, range: Range<usize>, window, cx| {
                                this.render_entries(range, window, cx)
                            }),
                        )
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll_handle)
                        .on_drop(cx.listener(
                            |this, dragged: &DraggedSessionEntry, window, cx| {
                                // Handle drops on the list area that didn't land on a specific item.
                                // If we have a valid drag_target, process the drop normally.
                                // Otherwise, clean up the drag state.
                                if this.drag_target.is_some() {
                                    this.handle_drop(dragged, window, cx);
                                } else {
                                    this.hover_expand_task = None;
                                    cx.notify();
                                }
                            },
                        ))
                        .into_any_element()
                    } else {
                        v_flex()
                            .size_full()
                            .p_4()
                            .gap_2()
                            .child(Label::new(t("remote_explorer.no_saved_sessions")).color(Color::Muted))
                            .into_any_element()
                    }),
            )
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(gpui::Corner::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

impl Focusable for RemoteExplorer {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for RemoteExplorer {
    fn persistent_name() -> &'static str {
        "Remote Explorer"
    }

    fn panel_key() -> &'static str {
        REMOTE_EXPLORER_PANEL_KEY
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
        self.width.unwrap_or(px(240.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Server)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("remote_explorer.title")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        0
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum NaturalSortSegment {
    Number(u64),
    Text(String),
}

fn natural_sort_key(name: &str) -> Vec<NaturalSortSegment> {
    let mut segments = Vec::new();
    let mut chars = name.chars().peekable();

    while let Some(&character) = chars.peek() {
        if character.is_ascii_digit() {
            let mut number_string = String::new();
            while let Some(&digit) = chars.peek() {
                if digit.is_ascii_digit() {
                    number_string.push(digit);
                    chars.next();
                } else {
                    break;
                }
            }
            let value = number_string.parse::<u64>().unwrap_or(0);
            segments.push(NaturalSortSegment::Number(value));
        } else {
            let mut text = String::new();
            while let Some(&character) = chars.peek() {
                if character.is_ascii_digit() {
                    break;
                }
                text.push(character);
                chars.next();
            }
            segments.push(NaturalSortSegment::Text(text));
        }
    }

    segments
}
