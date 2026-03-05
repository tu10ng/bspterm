use std::{cmp, ops::ControlFlow, path::PathBuf, process::ExitStatus, sync::Arc, time::Duration};

use crate::{
    TerminalView, default_working_directory,
    persistence::{
        SerializedItems, SerializedTerminalPanel, deserialize_terminal_panel, serialize_pane_group,
    },
    ssh_connect_modal::SshConnectModal,
};
use breadcrumbs::Breadcrumbs;
use collections::HashMap;
use db::kvp::KEY_VALUE_STORE;
use futures::{channel::oneshot, future::join_all};
use gpui::{
    Action, AnyView, App, AsyncApp, AsyncWindowContext, Context, Corner, Entity, EventEmitter,
    ExternalPaths, FocusHandle, Focusable, IntoElement, ParentElement, Pixels, Render, Styled,
    Task, WeakEntity, Window,
};
use itertools::Itertools;
use project::{Fs, Project, ProjectEntryId};

use settings::{Settings, TerminalDockPosition};
use task::{RevealStrategy, RevealTarget, Shell, ShellBuilder, SpawnInTerminal, TaskId};
use terminal::{Terminal, terminal_settings::TerminalSettings};
use ui::{
    ButtonLike, Clickable, ContextMenu, FluentBuilder, PopoverMenu, SplitButton, Toggleable,
    Tooltip, prelude::*,
};
use search;
use util::{ResultExt, TryFutureExt};
use workspace::{
    ActivateNextPane, ActivatePane, ActivatePaneDown, ActivatePaneLeft, ActivatePaneRight,
    ActivatePaneUp, ActivatePreviousPane, ConnectSsh, DraggedSelection, DraggedTab, ItemId,
    MoveItemToPane, MoveItemToPaneInDirection, MovePaneDown, MovePaneLeft, MovePaneRight,
    MovePaneUp, Pane, PaneGroup, SplitDirection, SplitDown, SplitLeft, SplitMode, SplitRight,
    SplitUp, SwapPaneDown, SwapPaneLeft, SwapPaneRight, SwapPaneUp, ToggleZoom, Workspace,
    dock::{DockPosition, Panel, PanelEvent, PanelHandle},
    item::SerializableItem,
    move_active_item, move_item, pane,
};

use anyhow::{Result, anyhow};
use bspterm_actions::assistant::InlineAssist;
pub use bspterm_actions::terminal_panel::{Toggle, ToggleFocus};
use i18n::t;

const TERMINAL_PANEL_KEY: &str = "TerminalPanel";

/// Key to identify a group of terminal tabs
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    /// A session group from Remote Explorer (by UUID)
    SessionGroup(uuid::Uuid),
    /// Root-level sessions (not in any group)
    Ungrouped,
    /// Local terminals (no connection info)
    Local,
    /// Non-terminal items (editors, buffers, etc.)
    Other,
}

/// A group of terminal tabs with their indices in the pane
#[derive(Clone, Debug)]
pub struct TerminalTabGroup {
    pub key: GroupKey,
    pub group_name: String,
    pub tab_indices: Vec<usize>,
}

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(TerminalPanel::new_terminal);
            workspace.register_action(TerminalPanel::open_terminal);
            workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
                if is_enabled_in_workspace(workspace, cx) {
                    workspace.toggle_panel_focus::<TerminalPanel>(window, cx);
                }
            });
            workspace.register_action(|workspace, _: &Toggle, window, cx| {
                if is_enabled_in_workspace(workspace, cx) {
                    if !workspace.toggle_panel_focus::<TerminalPanel>(window, cx) {
                        workspace.close_panel::<TerminalPanel>(window, cx);
                    }
                }
            });
            workspace.register_action(|workspace, _: &ConnectSsh, window, cx| {
                let pane = workspace.active_pane().clone();
                let weak_workspace = workspace.weak_handle();
                workspace.toggle_modal(window, cx, |window, cx| {
                    SshConnectModal::new(weak_workspace, pane, window, cx)
                });
            });
        },
    )
    .detach();
}

pub struct TerminalPanel {
    pub(crate) active_pane: Entity<Pane>,
    pub(crate) center: PaneGroup,
    fs: Arc<dyn Fs>,
    workspace: WeakEntity<Workspace>,
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
    pending_serialization: Task<Option<()>>,
    pending_terminals_to_add: usize,
    deferred_tasks: HashMap<TaskId, Task<()>>,
    assistant_enabled: bool,
    assistant_tab_bar_button: Option<AnyView>,
    active: bool,
    /// Order of tab groups. Groups appear in the order they were first opened.
    group_order: Vec<GroupKey>,
    /// Manual group overrides for non-terminal items (e.g., exported buffers)
    group_overrides: HashMap<gpui::EntityId, GroupKey>,
}

impl TerminalPanel {
    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = workspace.project();
        let pane = new_terminal_pane(workspace.weak_handle(), project.clone(), false, window, cx);
        let center = PaneGroup::new(pane.clone());
        let terminal_panel = Self {
            center,
            active_pane: pane,
            fs: workspace.app_state().fs.clone(),
            workspace: workspace.weak_handle(),
            pending_serialization: Task::ready(None),
            width: None,
            height: None,
            pending_terminals_to_add: 0,
            deferred_tasks: HashMap::default(),
            assistant_enabled: true,
            assistant_tab_bar_button: None,
            active: false,
            group_order: Vec::new(),
            group_overrides: HashMap::default(),
        };
        terminal_panel.apply_tab_bar_buttons(&terminal_panel.active_pane, cx);

        // Observe settings changes to re-apply tab bar when grouping setting changes
        cx.observe_global::<settings::SettingsStore>(|this, cx| {
            for pane in this.center.panes() {
                this.apply_tab_bar_buttons(pane, cx);
            }
            this.apply_grouped_tab_bar_to_center_panes(cx);
        })
        .detach();

        terminal_panel
    }

    pub fn set_assistant_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.assistant_enabled = enabled;
        if enabled {
            let focus_handle = self
                .active_pane
                .read(cx)
                .active_item()
                .map(|item| item.item_focus_handle(cx))
                .unwrap_or(self.focus_handle(cx));
            self.assistant_tab_bar_button = Some(
                cx.new(move |_| InlineAssistTabBarButton { focus_handle })
                    .into(),
            );
        } else {
            self.assistant_tab_bar_button = None;
        }
        for pane in self.center.panes() {
            self.apply_tab_bar_buttons(pane, cx);
        }
    }

    pub(crate) fn apply_tab_bar_buttons(&self, terminal_pane: &Entity<Pane>, cx: &mut Context<Self>) {
        let assistant_tab_bar_button = self.assistant_tab_bar_button.clone();
        let group_tabs = TerminalSettings::get_global(cx).group_tabs_by_session;
        let weak_panel = cx.entity().downgrade();

        terminal_pane.update(cx, |pane, cx| {
            // Set up the tab bar buttons (right side)
            pane.set_render_tab_bar_buttons(cx, move |pane, window, cx| {
                let split_context = pane
                    .active_item()
                    .and_then(|item| item.downcast::<TerminalView>())
                    .map(|terminal_view| terminal_view.read(cx).focus_handle.clone());
                let has_focused_rename_editor = pane
                    .active_item()
                    .and_then(|item| item.downcast::<TerminalView>())
                    .is_some_and(|view| view.read(cx).rename_editor_is_focused(window, cx));
                if !pane.has_focus(window, cx)
                    && !pane.context_menu_focused(window, cx)
                    && !has_focused_rename_editor
                {
                    return (None, None);
                }
                let focus_handle = pane.focus_handle(cx);
                let right_children = h_flex()
                    .gap(DynamicSpacing::Base02.rems(cx))
                    .child(
                        PopoverMenu::new("terminal-tab-bar-popover-menu")
                            .trigger_with_tooltip(
                                IconButton::new("plus", IconName::Plus).icon_size(IconSize::Small),
                                Tooltip::text(t("terminal_panel.new")),
                            )
                            .anchor(Corner::TopRight)
                            .with_handle(pane.new_item_context_menu_handle.clone())
                            .menu(move |window, cx| {
                                let focus_handle = focus_handle.clone();
                                let menu = ContextMenu::build(window, cx, |menu, _, _| {
                                    menu.context(focus_handle.clone())
                                        .action(
                                            t("terminal_panel.new_terminal"),
                                            workspace::NewTerminal::default().boxed_clone(),
                                        )
                                        // We want the focus to go back to terminal panel once task modal is dismissed,
                                        // hence we focus that first. Otherwise, we'd end up without a focused element, as
                                        // context menu will be gone the moment we spawn the modal.
                                        .action(
                                            t("terminal_panel.spawn_task"),
                                            bspterm_actions::Spawn::modal().boxed_clone(),
                                        )
                                });

                                Some(menu)
                            }),
                    )
                    .child(
                        IconButton::new("search", IconName::MagnifyingGlass)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(t("menu.find")))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(
                                    Box::new(search::buffer_search::Deploy::find()),
                                    cx,
                                );
                            }),
                    )
                    .children(assistant_tab_bar_button.clone())
                    .child(
                        PopoverMenu::new("terminal-pane-tab-bar-split")
                            .trigger_with_tooltip(
                                IconButton::new("terminal-pane-split", IconName::Split)
                                    .icon_size(IconSize::Small),
                                Tooltip::text(t("terminal_panel.split_terminal")),
                            )
                            .anchor(Corner::TopRight)
                            .with_handle(pane.split_item_context_menu_handle.clone())
                            .menu({
                                move |window, cx| {
                                    ContextMenu::build(window, cx, |menu, _, _| {
                                        menu.when_some(
                                            split_context.clone(),
                                            |menu, split_context| menu.context(split_context),
                                        )
                                        .action(t("menu.split_right"), SplitRight::default().boxed_clone())
                                        .action(t("menu.split_left"), SplitLeft::default().boxed_clone())
                                        .action(t("menu.split_up"), SplitUp::default().boxed_clone())
                                        .action(t("menu.split_down"), SplitDown::default().boxed_clone())
                                    })
                                    .into()
                                }
                            }),
                    )
                    .child({
                        let zoomed = pane.is_zoomed();
                        IconButton::new("toggle_zoom", IconName::Maximize)
                            .icon_size(IconSize::Small)
                            .toggle_state(zoomed)
                            .selected_icon(IconName::Minimize)
                            .on_click(cx.listener(|pane, _, window, cx| {
                                pane.toggle_zoom(&workspace::ToggleZoom, window, cx);
                            }))
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action(
                                    if zoomed { t("menu.zoom_out") } else { t("menu.zoom_in") },
                                    &ToggleZoom,
                                    cx,
                                )
                            })
                    })
                    .into_any_element()
                    .into();
                (None, right_children)
            });

            // Set up the grouped tab bar if enabled
            if group_tabs {
                let weak_panel = weak_panel.clone();
                pane.set_render_tab_bar(cx, move |pane, window, cx| {
                    render_grouped_tab_bar(&weak_panel, pane, window, cx)
                });
            } else {
                pane.set_render_tab_bar(cx, Pane::render_tab_bar);
            }
        });
    }

    fn apply_grouped_tab_bar_to_center_panes(&self, cx: &mut Context<Self>) {
        let group_tabs = TerminalSettings::get_global(cx).group_tabs_by_session;
        let weak_panel = cx.entity().downgrade();
        let Some(workspace) = self.workspace.upgrade() else { return };
        let center_panes: Vec<_> = workspace.read(cx).panes().to_vec();
        for pane in center_panes {
            pane.update(cx, |pane, cx| {
                if group_tabs {
                    let weak_panel = weak_panel.clone();
                    pane.set_render_tab_bar(cx, move |pane, window, cx| {
                        render_grouped_tab_bar(&weak_panel, pane, window, cx)
                    });
                } else {
                    pane.set_render_tab_bar(cx, Pane::render_tab_bar);
                }
            });
        }
    }

    pub fn register_item_group(&mut self, item_id: gpui::EntityId, group_key: GroupKey) {
        self.group_overrides.insert(item_id, group_key);
    }

    fn serialization_key(workspace: &Workspace) -> Option<String> {
        workspace
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or(workspace.session_id())
            .map(|id| format!("{:?}-{:?}", TERMINAL_PANEL_KEY, id))
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        let mut terminal_panel = None;

        if let Some((database_id, serialization_key)) = workspace
            .read_with(&cx, |workspace, _| {
                workspace
                    .database_id()
                    .zip(TerminalPanel::serialization_key(workspace))
            })
            .ok()
            .flatten()
            && let Some(serialized_panel) = cx
                .background_spawn(async move { KEY_VALUE_STORE.read_kvp(&serialization_key) })
                .await
                .log_err()
                .flatten()
                .map(|panel| serde_json::from_str::<SerializedTerminalPanel>(&panel))
                .transpose()
                .log_err()
                .flatten()
            && let Ok(serialized) = workspace
                .update_in(&mut cx, |workspace, window, cx| {
                    deserialize_terminal_panel(
                        workspace.weak_handle(),
                        workspace.project().clone(),
                        database_id,
                        serialized_panel,
                        window,
                        cx,
                    )
                })?
                .await
        {
            terminal_panel = Some(serialized);
        }

        let terminal_panel = if let Some(panel) = terminal_panel {
            panel
        } else {
            workspace.update_in(&mut cx, |workspace, window, cx| {
                cx.new(|cx| TerminalPanel::new(workspace, window, cx))
            })?
        };

        if let Some(workspace) = workspace.upgrade() {
            workspace.update(&mut cx, |workspace, _| {
                workspace.set_terminal_provider(TerminalProvider(terminal_panel.clone()))
            });
        }

        // Since panels/docks are loaded outside from the workspace, we cleanup here, instead of through the workspace.
        if let Some(workspace) = workspace.upgrade() {
            let cleanup_task = workspace.update_in(&mut cx, |workspace, window, cx| {
                let alive_item_ids = terminal_panel
                    .read(cx)
                    .center
                    .panes()
                    .into_iter()
                    .flat_map(|pane| pane.read(cx).items())
                    .map(|item| item.item_id().as_u64() as ItemId)
                    .collect();
                workspace.database_id().map(|workspace_id| {
                    TerminalView::cleanup(workspace_id, alive_item_ids, window, cx)
                })
            })?;
            if let Some(task) = cleanup_task {
                task.await.log_err();
            }
        }

        if let Some(workspace) = workspace.upgrade() {
            let should_focus = workspace
                .update_in(&mut cx, |workspace, window, cx| {
                    workspace.active_item(cx).is_none()
                        && workspace
                            .is_dock_at_position_open(terminal_panel.position(window, cx), cx)
                })
                .unwrap_or(false);

            if should_focus {
                terminal_panel
                    .update_in(&mut cx, |panel, window, cx| {
                        panel.active_pane.update(cx, |pane, cx| {
                            pane.focus_active_item(window, cx);
                        });
                    })
                    .ok();
            }
        }

        // Apply grouped tab bar to center panes
        terminal_panel
            .update_in(&mut cx, |panel, _window, cx| {
                panel.apply_grouped_tab_bar_to_center_panes(cx);
            })
            .ok();

        Ok(terminal_panel)
    }

    fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            pane::Event::ActivateItem { .. } => self.serialize(cx),
            pane::Event::RemovedItem { item } => {
                self.group_overrides.remove(&item.item_id());
                self.cleanup_empty_groups(pane, cx);
                self.serialize(cx);
            }
            pane::Event::Remove { focus_on_pane } => {
                let pane_count_before_removal = self.center.panes().len();
                let _removal_result = self.center.remove(pane, cx);
                if pane_count_before_removal == 1 {
                    self.center.first_pane().update(cx, |pane, cx| {
                        pane.set_zoomed(false, cx);
                    });
                    cx.emit(PanelEvent::Close);
                } else if let Some(focus_on_pane) =
                    focus_on_pane.as_ref().or_else(|| self.center.panes().pop())
                {
                    focus_on_pane.focus_handle(cx).focus(window, cx);
                }
            }
            pane::Event::ZoomIn => {
                for pane in self.center.panes() {
                    pane.update(cx, |pane, cx| {
                        pane.set_zoomed(true, cx);
                    })
                }
                cx.emit(PanelEvent::ZoomIn);
                cx.notify();
            }
            pane::Event::ZoomOut => {
                for pane in self.center.panes() {
                    pane.update(cx, |pane, cx| {
                        pane.set_zoomed(false, cx);
                    })
                }
                cx.emit(PanelEvent::ZoomOut);
                cx.notify();
            }
            pane::Event::AddItem { item } => {
                // Update group_order when new terminal is added
                if let Some(terminal_view) = item.downcast::<TerminalView>() {
                    self.update_group_order_for_terminal(&terminal_view.read(cx), cx);
                }
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        item.added_to_pane(workspace, pane.clone(), window, cx)
                    })
                }
                self.serialize(cx);
            }
            &pane::Event::Split { direction, mode } => {
                match mode {
                    SplitMode::ClonePane | SplitMode::EmptyPane => {
                        let clone = matches!(mode, SplitMode::ClonePane);
                        let new_pane = self.new_pane_with_active_terminal(clone, window, cx);
                        let pane = pane.clone();
                        cx.spawn_in(window, async move |panel, cx| {
                            let Some(new_pane) = new_pane.await else {
                                return;
                            };
                            panel
                                .update_in(cx, |panel, window, cx| {
                                    panel
                                        .center
                                        .split(&pane, &new_pane, direction, cx)
                                        .log_err();
                                    window.focus(&new_pane.focus_handle(cx), cx);
                                })
                                .ok();
                        })
                        .detach();
                    }
                    SplitMode::MovePane => {
                        let Some(item) =
                            pane.update(cx, |pane, cx| pane.take_active_item(window, cx))
                        else {
                            return;
                        };
                        let Ok(project) = self
                            .workspace
                            .update(cx, |workspace, _| workspace.project().clone())
                        else {
                            return;
                        };
                        let new_pane =
                            new_terminal_pane(self.workspace.clone(), project, false, window, cx);
                        self.apply_tab_bar_buttons(&new_pane, cx);
                        new_pane.update(cx, |pane, cx| {
                            pane.add_item(item, true, true, None, window, cx);
                        });
                        self.center.split(&pane, &new_pane, direction, cx).log_err();
                        window.focus(&new_pane.focus_handle(cx), cx);
                    }
                };
            }
            pane::Event::Focus => {
                self.active_pane = pane.clone();
            }
            pane::Event::ItemPinned | pane::Event::ItemUnpinned => {
                self.serialize(cx);
            }

            _ => {}
        }
    }

    fn new_pane_with_active_terminal(
        &mut self,
        clone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Pane>>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(None);
        };
        let workspace = workspace.read(cx);
        let database_id = workspace.database_id();
        let weak_workspace = self.workspace.clone();
        let project = workspace.project().clone();
        let active_pane = &self.active_pane;
        let terminal_view = if clone {
            active_pane
                .read(cx)
                .active_item()
                .and_then(|item| item.downcast::<TerminalView>())
        } else {
            None
        };
        let working_directory = if clone {
            terminal_view
                .as_ref()
                .and_then(|terminal_view| {
                    terminal_view
                        .read(cx)
                        .terminal()
                        .read(cx)
                        .working_directory()
                })
                .or_else(|| default_working_directory(workspace, cx))
        } else {
            default_working_directory(workspace, cx)
        };

        let is_zoomed = if clone {
            active_pane.read(cx).is_zoomed()
        } else {
            false
        };
        cx.spawn_in(window, async move |panel, cx| {
            let terminal = project
                .update(cx, |project, cx| match terminal_view {
                    Some(view) => project.clone_terminal(
                        &view.read(cx).terminal.clone(),
                        cx,
                        working_directory,
                    ),
                    None => project.create_terminal_shell(working_directory, cx),
                })
                .await
                .log_err()?;

            panel
                .update_in(cx, move |terminal_panel, window, cx| {
                    let terminal_view = Box::new(cx.new(|cx| {
                        TerminalView::new(
                            terminal.clone(),
                            weak_workspace.clone(),
                            database_id,
                            project.downgrade(),
                            window,
                            cx,
                        )
                    }));
                    let pane = new_terminal_pane(weak_workspace, project, is_zoomed, window, cx);
                    terminal_panel.apply_tab_bar_buttons(&pane, cx);
                    pane.update(cx, |pane, cx| {
                        pane.add_item(terminal_view, true, true, None, window, cx);
                    });
                    Some(pane)
                })
                .ok()
                .flatten()
        })
    }

    pub fn open_terminal(
        workspace: &mut Workspace,
        action: &workspace::OpenTerminal,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(terminal_panel) = workspace.panel::<Self>(cx) else {
            return;
        };

        terminal_panel
            .update(cx, |panel, cx| {
                if action.local {
                    panel.add_local_terminal_shell(RevealStrategy::Always, window, cx)
                } else {
                    panel.add_terminal_shell(
                        Some(action.working_directory.clone()),
                        RevealStrategy::Always,
                        window,
                        cx,
                    )
                }
            })
            .detach_and_log_err(cx);
    }

    pub fn spawn_task(
        &mut self,
        task: &SpawnInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Err(anyhow!("failed to read workspace")));
        };

        let project = workspace.read(cx).project().read(cx);

        if project.is_via_collab() {
            return Task::ready(Err(anyhow!("cannot spawn tasks as a guest")));
        }

        let remote_client = project.remote_client();
        let is_windows = project.path_style(cx).is_windows();
        let remote_shell = remote_client
            .as_ref()
            .and_then(|remote_client| remote_client.read(cx).shell());

        let shell = if let Some(remote_shell) = remote_shell
            && task.shell == Shell::System
        {
            Shell::Program(remote_shell)
        } else {
            task.shell.clone()
        };

        let task = prepare_task_for_spawn(task, &shell, is_windows);

        if task.allow_concurrent_runs && task.use_new_terminal {
            return self.spawn_in_new_terminal(task, window, cx);
        }

        let mut terminals_for_task = self.terminals_for_task(&task.full_label, cx);
        let Some(existing) = terminals_for_task.pop() else {
            return self.spawn_in_new_terminal(task, window, cx);
        };

        let (existing_item_index, task_pane, existing_terminal) = existing;
        if task.allow_concurrent_runs {
            return self.replace_terminal(
                task,
                task_pane,
                existing_item_index,
                existing_terminal,
                window,
                cx,
            );
        }

        let (tx, rx) = oneshot::channel();

        self.deferred_tasks.insert(
            task.id.clone(),
            cx.spawn_in(window, async move |terminal_panel, cx| {
                wait_for_terminals_tasks(terminals_for_task, cx).await;
                let task = terminal_panel.update_in(cx, |terminal_panel, window, cx| {
                    if task.use_new_terminal {
                        terminal_panel.spawn_in_new_terminal(task, window, cx)
                    } else {
                        terminal_panel.replace_terminal(
                            task,
                            task_pane,
                            existing_item_index,
                            existing_terminal,
                            window,
                            cx,
                        )
                    }
                });
                if let Ok(task) = task {
                    tx.send(task.await).ok();
                }
            }),
        );

        cx.spawn(async move |_, _| rx.await?)
    }

    fn spawn_in_new_terminal(
        &mut self,
        spawn_task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let reveal = spawn_task.reveal;
        let reveal_target = spawn_task.reveal_target;
        match reveal_target {
            RevealTarget::Center => self
                .workspace
                .update(cx, |workspace, cx| {
                    Self::add_center_terminal(workspace, window, cx, |project, cx| {
                        project.create_terminal_task(spawn_task, cx)
                    })
                })
                .unwrap_or_else(|e| Task::ready(Err(e))),
            RevealTarget::Dock => self.add_terminal_task(spawn_task, reveal, window, cx),
        }
    }

    /// Create a new Terminal in the current working directory or the user's home directory
    fn new_terminal(
        workspace: &mut Workspace,
        action: &workspace::NewTerminal,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(terminal_panel) = workspace.panel::<Self>(cx) else {
            return;
        };

        terminal_panel
            .update(cx, |this, cx| {
                if action.local {
                    this.add_local_terminal_shell(RevealStrategy::Always, window, cx)
                } else {
                    this.add_terminal_shell(
                        default_working_directory(workspace, cx),
                        RevealStrategy::Always,
                        window,
                        cx,
                    )
                }
            })
            .detach_and_log_err(cx);
    }

    fn terminals_for_task(
        &self,
        label: &str,
        cx: &mut App,
    ) -> Vec<(usize, Entity<Pane>, Entity<TerminalView>)> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };

        let pane_terminal_views = |pane: Entity<Pane>| {
            pane.read(cx)
                .items()
                .enumerate()
                .filter_map(|(index, item)| Some((index, item.act_as::<TerminalView>(cx)?)))
                .filter_map(|(index, terminal_view)| {
                    let task_state = terminal_view.read(cx).terminal().read(cx).task()?;
                    if &task_state.spawned_task.full_label == label {
                        Some((index, terminal_view))
                    } else {
                        None
                    }
                })
                .map(move |(index, terminal_view)| (index, pane.clone(), terminal_view))
        };

        self.center
            .panes()
            .into_iter()
            .cloned()
            .flat_map(pane_terminal_views)
            .chain(
                workspace
                    .read(cx)
                    .panes()
                    .iter()
                    .cloned()
                    .flat_map(pane_terminal_views),
            )
            .sorted_by_key(|(_, _, terminal_view)| terminal_view.entity_id())
            .collect()
    }

    fn activate_terminal_view(
        &self,
        pane: &Entity<Pane>,
        item_index: usize,
        focus: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        pane.update(cx, |pane, cx| {
            pane.activate_item(item_index, true, focus, window, cx)
        })
    }

    pub fn add_center_terminal(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
        create_terminal: impl FnOnce(
            &mut Project,
            &mut Context<Project>,
        ) -> Task<Result<Entity<Terminal>>>
        + 'static,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        if !is_enabled_in_workspace(workspace, cx) {
            return Task::ready(Err(anyhow!(
                "terminal not yet supported for remote projects"
            )));
        }
        let project = workspace.project().downgrade();
        cx.spawn_in(window, async move |workspace, cx| {
            let terminal = project.update(cx, create_terminal)?.await?;

            workspace.update_in(cx, |workspace, window, cx| {
                let terminal_view = cx.new(|cx| {
                    TerminalView::new(
                        terminal.clone(),
                        workspace.weak_handle(),
                        workspace.database_id(),
                        workspace.project().downgrade(),
                        window,
                        cx,
                    )
                });
                workspace.add_item_to_active_pane(Box::new(terminal_view), None, true, window, cx);
            })?;
            Ok(terminal.downgrade())
        })
    }

    pub fn add_terminal_task(
        &mut self,
        task: SpawnInTerminal,
        reveal_strategy: RevealStrategy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |terminal_panel, cx| {
            if workspace.update(cx, |workspace, cx| !is_enabled_in_workspace(workspace, cx))? {
                anyhow::bail!("terminal not yet supported for remote projects");
            }
            let pane = terminal_panel.update(cx, |terminal_panel, _| {
                terminal_panel.pending_terminals_to_add += 1;
                terminal_panel.active_pane.clone()
            })?;
            let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;
            let terminal = project
                .update(cx, |project, cx| project.create_terminal_task(task, cx))
                .await?;
            let result = workspace.update_in(cx, |workspace, window, cx| {
                let terminal_view = Box::new(cx.new(|cx| {
                    TerminalView::new(
                        terminal.clone(),
                        workspace.weak_handle(),
                        workspace.database_id(),
                        workspace.project().downgrade(),
                        window,
                        cx,
                    )
                }));

                match reveal_strategy {
                    RevealStrategy::Always => {
                        workspace.focus_panel::<Self>(window, cx);
                    }
                    RevealStrategy::NoFocus => {
                        workspace.open_panel::<Self>(window, cx);
                    }
                    RevealStrategy::Never => {}
                }

                pane.update(cx, |pane, cx| {
                    let focus = matches!(reveal_strategy, RevealStrategy::Always);
                    pane.add_item(terminal_view, true, focus, None, window, cx);
                });

                Ok(terminal.downgrade())
            })?;
            terminal_panel.update(cx, |terminal_panel, cx| {
                terminal_panel.pending_terminals_to_add =
                    terminal_panel.pending_terminals_to_add.saturating_sub(1);
                terminal_panel.serialize(cx)
            })?;
            result
        })
    }

    fn add_terminal_shell(
        &mut self,
        cwd: Option<PathBuf>,
        reveal_strategy: RevealStrategy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        self.add_terminal_shell_internal(false, cwd, reveal_strategy, window, cx)
    }

    fn add_local_terminal_shell(
        &mut self,
        reveal_strategy: RevealStrategy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        self.add_terminal_shell_internal(true, None, reveal_strategy, window, cx)
    }

    fn add_terminal_shell_internal(
        &mut self,
        force_local: bool,
        cwd: Option<PathBuf>,
        reveal_strategy: RevealStrategy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let workspace = self.workspace.clone();

        cx.spawn_in(window, async move |terminal_panel, cx| {
            if workspace.update(cx, |workspace, cx| !is_enabled_in_workspace(workspace, cx))? {
                anyhow::bail!("terminal not yet supported for collaborative projects");
            }
            let pane = terminal_panel.update(cx, |terminal_panel, _| {
                terminal_panel.pending_terminals_to_add += 1;
                terminal_panel.active_pane.clone()
            })?;
            let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;
            let terminal = if force_local {
                project
                    .update(cx, |project, cx| project.create_local_terminal(cx))
                    .await
            } else {
                project
                    .update(cx, |project, cx| project.create_terminal_shell(cwd, cx))
                    .await
            };

            match terminal {
                Ok(terminal) => {
                    let result = workspace.update_in(cx, |workspace, window, cx| {
                        let terminal_view = Box::new(cx.new(|cx| {
                            TerminalView::new(
                                terminal.clone(),
                                workspace.weak_handle(),
                                workspace.database_id(),
                                workspace.project().downgrade(),
                                window,
                                cx,
                            )
                        }));

                        match reveal_strategy {
                            RevealStrategy::Always => {
                                workspace.focus_panel::<Self>(window, cx);
                            }
                            RevealStrategy::NoFocus => {
                                workspace.open_panel::<Self>(window, cx);
                            }
                            RevealStrategy::Never => {}
                        }

                        pane.update(cx, |pane, cx| {
                            let focus = matches!(reveal_strategy, RevealStrategy::Always);
                            pane.add_item(terminal_view, true, focus, None, window, cx);
                        });

                        Ok(terminal.downgrade())
                    })?;
                    terminal_panel.update(cx, |terminal_panel, cx| {
                        terminal_panel.pending_terminals_to_add =
                            terminal_panel.pending_terminals_to_add.saturating_sub(1);
                        terminal_panel.serialize(cx)
                    })?;
                    result
                }
                Err(error) => {
                    pane.update_in(cx, |pane, window, cx| {
                        let focus = pane.has_focus(window, cx);
                        let failed_to_spawn = cx.new(|cx| FailedToSpawnTerminal {
                            error: error.to_string(),
                            focus_handle: cx.focus_handle(),
                        });
                        pane.add_item(Box::new(failed_to_spawn), true, focus, None, window, cx);
                    })?;
                    Err(error)
                }
            }
        })
    }

    fn serialize(&mut self, cx: &mut Context<Self>) {
        let height = self.height;
        let width = self.width;
        let Some(serialization_key) = self
            .workspace
            .read_with(cx, |workspace, _| {
                TerminalPanel::serialization_key(workspace)
            })
            .ok()
            .flatten()
        else {
            return;
        };
        self.pending_serialization = cx.spawn(async move |terminal_panel, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            let terminal_panel = terminal_panel.upgrade()?;
            let items = terminal_panel.update(cx, |terminal_panel, cx| {
                SerializedItems::WithSplits(serialize_pane_group(
                    &terminal_panel.center,
                    &terminal_panel.active_pane,
                    cx,
                ))
            });
            cx.background_spawn(
                async move {
                    KEY_VALUE_STORE
                        .write_kvp(
                            serialization_key,
                            serde_json::to_string(&SerializedTerminalPanel {
                                items,
                                active_item_id: None,
                                height,
                                width,
                            })?,
                        )
                        .await?;
                    anyhow::Ok(())
                }
                .log_err(),
            )
            .await;
            Some(())
        });
    }

    fn replace_terminal(
        &self,
        spawn_task: SpawnInTerminal,
        task_pane: Entity<Pane>,
        terminal_item_index: usize,
        terminal_to_replace: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let reveal = spawn_task.reveal;
        let task_workspace = self.workspace.clone();
        cx.spawn_in(window, async move |terminal_panel, cx| {
            let project = terminal_panel.update(cx, |this, cx| {
                this.workspace
                    .update(cx, |workspace, _| workspace.project().clone())
            })??;
            let new_terminal = project
                .update(cx, |project, cx| {
                    project.create_terminal_task(spawn_task, cx)
                })
                .await?;
            terminal_to_replace.update_in(cx, |terminal_to_replace, window, cx| {
                terminal_to_replace.set_terminal(new_terminal.clone(), window, cx);
            })?;

            let reveal_target = terminal_panel.update(cx, |panel, _| {
                if panel.center.panes().iter().any(|p| **p == task_pane) {
                    RevealTarget::Dock
                } else {
                    RevealTarget::Center
                }
            })?;

            match reveal {
                RevealStrategy::Always => match reveal_target {
                    RevealTarget::Center => {
                        task_workspace.update_in(cx, |workspace, window, cx| {
                            let did_activate = workspace.activate_item(
                                &terminal_to_replace,
                                true,
                                true,
                                window,
                                cx,
                            );

                            anyhow::ensure!(did_activate, "Failed to retrieve terminal pane");

                            anyhow::Ok(())
                        })??;
                    }
                    RevealTarget::Dock => {
                        terminal_panel.update_in(cx, |terminal_panel, window, cx| {
                            terminal_panel.activate_terminal_view(
                                &task_pane,
                                terminal_item_index,
                                true,
                                window,
                                cx,
                            )
                        })?;

                        cx.spawn(async move |cx| {
                            task_workspace
                                .update_in(cx, |workspace, window, cx| {
                                    workspace.focus_panel::<Self>(window, cx)
                                })
                                .ok()
                        })
                        .detach();
                    }
                },
                RevealStrategy::NoFocus => match reveal_target {
                    RevealTarget::Center => {
                        task_workspace.update_in(cx, |workspace, window, cx| {
                            workspace.active_pane().focus_handle(cx).focus(window, cx);
                        })?;
                    }
                    RevealTarget::Dock => {
                        terminal_panel.update_in(cx, |terminal_panel, window, cx| {
                            terminal_panel.activate_terminal_view(
                                &task_pane,
                                terminal_item_index,
                                false,
                                window,
                                cx,
                            )
                        })?;

                        cx.spawn(async move |cx| {
                            task_workspace
                                .update_in(cx, |workspace, window, cx| {
                                    workspace.open_panel::<Self>(window, cx)
                                })
                                .ok()
                        })
                        .detach();
                    }
                },
                RevealStrategy::Never => {}
            }

            Ok(new_terminal.downgrade())
        })
    }

    fn has_no_terminals(&self, cx: &App) -> bool {
        self.active_pane.read(cx).items_len() == 0 && self.pending_terminals_to_add == 0
    }

    pub fn assistant_enabled(&self) -> bool {
        self.assistant_enabled
    }

    /// Returns all panes in the terminal panel.
    pub fn panes(&self) -> Vec<&Entity<Pane>> {
        self.center.panes()
    }

    /// Returns all non-empty terminal selections from all terminal views in all panes.
    pub fn terminal_selections(&self, cx: &App) -> Vec<String> {
        self.center
            .panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx).items().filter_map(|item| {
                    let terminal_view = item.downcast::<crate::TerminalView>()?;
                    terminal_view
                        .read(cx)
                        .terminal()
                        .read(cx)
                        .last_content
                        .selection_text
                        .clone()
                        .filter(|text| !text.is_empty())
                })
            })
            .collect()
    }

    fn is_enabled(&self, cx: &App) -> bool {
        self.workspace
            .upgrade()
            .is_some_and(|workspace| is_enabled_in_workspace(workspace.read(cx), cx))
    }

    fn activate_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
        {
            window.focus(&pane.focus_handle(cx), cx);
        } else {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.activate_pane_in_direction(direction, window, cx)
                })
                .ok();
        }
    }

    fn swap_pane_in_direction(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        if let Some(to) = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
            .cloned()
        {
            self.center.swap(&self.active_pane, &to, cx);
            cx.notify();
        }
    }

    fn move_pane_to_border(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        if self
            .center
            .move_to_border(&self.active_pane, direction, cx)
            .unwrap()
        {
            cx.notify();
        }
    }

    /// Convert a terminal view to its group key
    fn terminal_to_group_key(&self, terminal_view: &TerminalView, cx: &App) -> GroupKey {
        let (group_id, _) = terminal_view.get_session_group_info(cx);

        // Check if it's a remote terminal (SSH/Telnet)
        let is_remote = terminal_view
            .terminal()
            .read(cx)
            .connection_info()
            .is_some();

        match group_id {
            Some(id) => GroupKey::SessionGroup(id),
            None if is_remote => GroupKey::Ungrouped,
            None => GroupKey::Local,
        }
    }

    /// Groups terminal items by session group.
    /// Returns groups in the order stored in `self.group_order`.
    fn group_terminals_by_session(&self, pane: &Pane, cx: &App) -> Vec<TerminalTabGroup> {
        let mut groups: HashMap<GroupKey, TerminalTabGroup> = HashMap::default();

        // Collect all terminals into groups
        for (index, item) in pane.items().enumerate() {
            if let Some(terminal_view) = item.downcast::<TerminalView>() {
                let terminal_view = terminal_view.read(cx);
                let key = self.terminal_to_group_key(terminal_view, cx);
                let (_, group_name) = terminal_view.get_session_group_info(cx);

                let group_name = match &key {
                    GroupKey::SessionGroup(_) => group_name.unwrap_or_else(|| "Unknown".to_string()),
                    GroupKey::Ungrouped => "Ungrouped".to_string(),
                    GroupKey::Local => "Local".to_string(),
                    GroupKey::Other => "Other".to_string(),
                };

                groups
                    .entry(key.clone())
                    .or_insert_with(|| TerminalTabGroup {
                        key: key.clone(),
                        group_name,
                        tab_indices: Vec::new(),
                    })
                    .tab_indices
                    .push(index);
            } else {
                let item_id = item.item_id();
                let key = self.group_overrides.get(&item_id).cloned()
                    .unwrap_or(GroupKey::Other);

                let group_name = match &key {
                    GroupKey::SessionGroup(_) => {
                        // Look up group name from any terminal in that group
                        "Unknown".to_string()
                    }
                    GroupKey::Ungrouped => "Ungrouped".to_string(),
                    GroupKey::Local => "Local".to_string(),
                    GroupKey::Other => "Other".to_string(),
                };

                groups
                    .entry(key.clone())
                    .or_insert_with(|| TerminalTabGroup {
                        key: key.clone(),
                        group_name,
                        tab_indices: Vec::new(),
                    })
                    .tab_indices
                    .push(index);
            }
        }

        // Order groups according to self.group_order, keeping Other always at the bottom
        let mut result = Vec::new();
        let mut other_group = None;

        for key in &self.group_order {
            if let Some(group) = groups.remove(key) {
                if group.key == GroupKey::Other {
                    other_group = Some(group);
                } else {
                    result.push(group);
                }
            }
        }

        // Append any new groups not yet in order (preserving encounter order from pane items)
        for group in groups.into_values() {
            if group.key == GroupKey::Other {
                other_group = Some(group);
            } else {
                result.push(group);
            }
        }

        if let Some(group) = other_group {
            result.push(group);
        }

        result
    }

    /// Called when a terminal is added - updates group_order if needed
    fn update_group_order_for_terminal(&mut self, terminal_view: &TerminalView, cx: &App) {
        let key = self.terminal_to_group_key(terminal_view, cx);

        if !self.group_order.contains(&key) {
            self.group_order.push(key);
        }
    }

    /// Remove empty groups from group_order
    fn cleanup_empty_groups(&mut self, pane: &Entity<Pane>, cx: &App) {
        let groups = self.group_terminals_by_session(pane.read(cx), cx);
        let active_keys: std::collections::HashSet<_> = groups.iter().map(|g| &g.key).collect();
        self.group_order.retain(|key| active_keys.contains(key));
    }

    /// Move a group row up in the order
    fn move_group_up(&mut self, key: &GroupKey, cx: &mut Context<Self>) {
        if let Some(pos) = self.group_order.iter().position(|k| k == key) {
            if pos > 0 {
                self.group_order.swap(pos, pos - 1);
                cx.notify();
            }
        }
    }

    /// Move a group row down in the order
    fn move_group_down(&mut self, key: &GroupKey, cx: &mut Context<Self>) {
        if let Some(pos) = self.group_order.iter().position(|k| k == key) {
            if pos < self.group_order.len() - 1 {
                self.group_order.swap(pos, pos + 1);
                cx.notify();
            }
        }
    }

    /// Close all terminals in a group
    fn close_group(
        &mut self,
        key: &GroupKey,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let groups = self.group_terminals_by_session(pane.read(cx), cx);
        if let Some(group) = groups.iter().find(|g| &g.key == key) {
            // Collect item IDs first, then close them (to avoid index shifting issues)
            let item_ids: Vec<_> = group
                .tab_indices
                .iter()
                .filter_map(|&ix| pane.read(cx).item_for_index(ix).map(|item| item.item_id()))
                .collect();
            for item_id in item_ids.into_iter().rev() {
                pane.update(cx, |pane, cx| {
                    pane.close_item_by_id(item_id, workspace::SaveIntent::Close, window, cx)
                        .detach_and_log_err(cx);
                });
            }
        }
        // Remove from group_order
        self.group_order.retain(|k| k != key);
        cx.notify();
    }
}

/// Render a grouped tab bar where tabs are organized by session group.
/// Each group gets its own row with: [GroupName] [Tabs...] [↑][↓][×]
fn render_grouped_tab_bar(
    weak_panel: &WeakEntity<TerminalPanel>,
    pane: &mut Pane,
    window: &mut Window,
    cx: &mut Context<Pane>,
) -> gpui::AnyElement {
    let Some(panel) = weak_panel.upgrade() else {
        return gpui::Empty.into_any();
    };

    let groups = panel.read(cx).group_terminals_by_session(pane, cx);

    if groups.is_empty() {
        return Pane::render_tab_bar(pane, window, cx);
    }

    let focus_handle = pane.focus_handle(cx);
    let total_groups = groups.len();
    let active_item_index = pane.active_item_index();

    v_flex()
        .w_full()
        .bg(cx.theme().colors().tab_bar_background)
        .children(groups.into_iter().enumerate().map(|(row_idx, group)| {
            let is_first = row_idx == 0;
            let is_last = row_idx == total_groups - 1;
            let group_key = group.key.clone();
            let weak_panel = weak_panel.clone();
            let pane_entity = cx.entity();

            // Single row: [GroupLabel] [Tabs...] [Controls]
            h_flex()
                .w_full()
                .min_h(px(29.))
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .drag_over::<DraggedTab>(|row, _, _, cx| {
                    row.bg(cx.theme().colors().drop_target_background)
                })
                .on_drop({
                    let group_key = group_key.clone();
                    let weak_panel = weak_panel.clone();
                    move |dragged_tab: &DraggedTab, _window, cx| {
                        let item_id = dragged_tab.item.item_id();
                        if let Some(panel) = weak_panel.upgrade() {
                            panel.update(cx, |panel, cx| {
                                panel.register_item_group(item_id, group_key.clone());
                                cx.notify();
                            });
                        }
                    }
                })
                // Group name label (fixed width prefix)
                .child(
                    div()
                        .px_2()
                        .flex_shrink_0()
                        .min_w(px(80.))
                        .max_w(px(120.))
                        .border_r_1()
                        .border_color(cx.theme().colors().border)
                        .flex()
                        .items_center()
                        .child(
                            Label::new(group.group_name.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .truncate()
                        )
                )
                // Tabs (flexible, scrollable if needed)
                .child(
                    div()
                        .flex_1()
                        .overflow_x_hidden()
                        .child(
                            h_flex()
                                .children(group.tab_indices.iter().map(|&ix| {
                                    let item = pane.item_for_index(ix);
                                    let is_active = ix == active_item_index;
                                    if let Some(item) = item {
                                        let tab = pane.render_tab(
                                            ix,
                                            item,
                                            0,
                                            &focus_handle,
                                            false,
                                            window,
                                            cx,
                                        );
                                        // Wrap in a div to apply active styling
                                        div()
                                            .when(is_active, |d| d.bg(cx.theme().colors().tab_active_background))
                                            .child(tab)
                                            .into_any_element()
                                    } else {
                                        gpui::Empty.into_any()
                                    }
                                }))
                        )
                )
                // Row control buttons (fixed width suffix)
                .child({
                    let group_key_up = group_key.clone();
                    let group_key_down = group_key.clone();
                    let group_key_close = group_key.clone();
                    let weak_panel_up = weak_panel.clone();
                    let weak_panel_down = weak_panel.clone();
                    let weak_panel_close = weak_panel.clone();
                    let pane_entity_close = pane_entity.clone();

                    h_flex()
                        .flex_shrink_0()
                        .px_1()
                        .gap_0p5()
                        .border_l_1()
                        .border_color(cx.theme().colors().border)
                        .child(
                            IconButton::new(
                                SharedString::from(format!("move_up_{}", row_idx)),
                                IconName::ChevronUp,
                            )
                            .icon_size(IconSize::XSmall)
                            .disabled(is_first)
                            .tooltip(Tooltip::text(t("terminal_panel.move_group_up")))
                            .on_click(move |_, _, cx| {
                                if let Some(panel) = weak_panel_up.upgrade() {
                                    panel.update(cx, |panel, cx| {
                                        panel.move_group_up(&group_key_up, cx);
                                    });
                                }
                            })
                        )
                        .child(
                            IconButton::new(
                                SharedString::from(format!("move_down_{}", row_idx)),
                                IconName::ChevronDown,
                            )
                            .icon_size(IconSize::XSmall)
                            .disabled(is_last)
                            .tooltip(Tooltip::text(t("terminal_panel.move_group_down")))
                            .on_click(move |_, _, cx| {
                                if let Some(panel) = weak_panel_down.upgrade() {
                                    panel.update(cx, |panel, cx| {
                                        panel.move_group_down(&group_key_down, cx);
                                    });
                                }
                            })
                        )
                        .child(
                            IconButton::new(
                                SharedString::from(format!("close_group_{}", row_idx)),
                                IconName::Close,
                            )
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text(t("terminal_panel.close_group")))
                            .on_click(move |_, window, cx| {
                                if let Some(panel) = weak_panel_close.upgrade() {
                                    panel.update(cx, |panel, cx| {
                                        panel.close_group(&group_key_close, &pane_entity_close, window, cx);
                                    });
                                }
                            })
                        )
                })
        }))
        .into_any_element()
}

/// Prepares a `SpawnInTerminal` by computing the command, args, and command_label
/// based on the shell configuration. This is a pure function that can be tested
/// without spawning actual terminals.
pub fn prepare_task_for_spawn(
    task: &SpawnInTerminal,
    shell: &Shell,
    is_windows: bool,
) -> SpawnInTerminal {
    let builder = ShellBuilder::new(shell, is_windows);
    let command_label = builder.command_label(task.command.as_deref().unwrap_or(""));
    let (command, args) = builder.build_no_quote(task.command.clone(), &task.args);

    SpawnInTerminal {
        command_label,
        command: Some(command),
        args,
        ..task.clone()
    }
}

fn is_enabled_in_workspace(workspace: &Workspace, cx: &App) -> bool {
    workspace.project().read(cx).supports_terminal(cx)
}

pub fn new_terminal_pane(
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    zoomed: bool,
    window: &mut Window,
    cx: &mut Context<TerminalPanel>,
) -> Entity<Pane> {
    let is_local = project.read(cx).is_local();
    let terminal_panel = cx.entity();
    let pane = cx.new(|cx| {
        let mut pane = Pane::new(
            workspace.clone(),
            project.clone(),
            Default::default(),
            None,
            workspace::NewTerminal::default().boxed_clone(),
            false,
            window,
            cx,
        );
        pane.set_zoomed(zoomed, cx);
        pane.set_can_navigate(false, cx);
        pane.display_nav_history_buttons(None);
        pane.set_should_display_tab_bar(|_, _| true);
        pane.set_zoom_out_on_close(false);

        let split_closure_terminal_panel = terminal_panel.downgrade();
        pane.set_can_split(Some(Arc::new(move |pane, dragged_item, _window, cx| {
            if let Some(tab) = dragged_item.downcast_ref::<DraggedTab>() {
                let is_current_pane = tab.pane == cx.entity();
                let Some(can_drag_away) = split_closure_terminal_panel
                    .read_with(cx, |terminal_panel, _| {
                        let current_panes = terminal_panel.center.panes();
                        !current_panes.contains(&&tab.pane)
                            || current_panes.len() > 1
                            || (!is_current_pane || pane.items_len() > 1)
                    })
                    .ok()
                else {
                    return false;
                };
                if can_drag_away {
                    let item = if is_current_pane {
                        pane.item_for_index(tab.ix)
                    } else {
                        tab.pane.read(cx).item_for_index(tab.ix)
                    };
                    if let Some(item) = item {
                        return item.downcast::<TerminalView>().is_some();
                    }
                }
            }
            false
        })));

        let toolbar = pane.toolbar().clone();
        if let Some(callbacks) = cx.try_global::<workspace::PaneSearchBarCallbacks>() {
            let languages = Some(project.read(cx).languages().clone());
            (callbacks.setup_search_bar)(languages, &toolbar, window, cx);
        }
        let breadcrumbs = cx.new(|_| Breadcrumbs::new());
        toolbar.update(cx, |toolbar, cx| {
            toolbar.add_item(breadcrumbs, window, cx);
        });

        let drop_closure_project = project.downgrade();
        let drop_closure_terminal_panel = terminal_panel.downgrade();
        pane.set_custom_drop_handle(cx, move |pane, dropped_item, window, cx| {
            let Some(project) = drop_closure_project.upgrade() else {
                return ControlFlow::Break(());
            };
            if let Some(tab) = dropped_item.downcast_ref::<DraggedTab>() {
                let this_pane = cx.entity();
                let item = if tab.pane == this_pane {
                    pane.item_for_index(tab.ix)
                } else {
                    tab.pane.read(cx).item_for_index(tab.ix)
                };
                if let Some(item) = item {
                    if item.downcast::<TerminalView>().is_some() {
                        let source = tab.pane.clone();
                        let item_id_to_move = item.item_id();

                        // If no split direction, let the regular pane drop handler take care of it
                        let Some(split_direction) = pane.drag_split_direction() else {
                            return ControlFlow::Continue(());
                        };

                        // Gather data synchronously before deferring
                        let is_zoomed = drop_closure_terminal_panel
                            .upgrade()
                            .map(|terminal_panel| {
                                let terminal_panel = terminal_panel.read(cx);
                                if terminal_panel.active_pane == this_pane {
                                    pane.is_zoomed()
                                } else {
                                    terminal_panel.active_pane.read(cx).is_zoomed()
                                }
                            })
                            .unwrap_or(false);

                        let workspace = workspace.clone();
                        let terminal_panel = drop_closure_terminal_panel.clone();

                        // Defer the split operation to avoid re-entrancy panic.
                        // The pane may be the one currently being updated, so we cannot
                        // call mark_positions (via split) synchronously.
                        cx.spawn_in(window, async move |_, cx| {
                            cx.update(|window, cx| {
                                let Ok(new_pane) =
                                    terminal_panel.update(cx, |terminal_panel, cx| {
                                        let new_pane = new_terminal_pane(
                                            workspace, project, is_zoomed, window, cx,
                                        );
                                        terminal_panel.apply_tab_bar_buttons(&new_pane, cx);
                                        terminal_panel.center.split(
                                            &this_pane,
                                            &new_pane,
                                            split_direction,
                                            cx,
                                        )?;
                                        anyhow::Ok(new_pane)
                                    })
                                else {
                                    return;
                                };

                                let Some(new_pane) = new_pane.log_err() else {
                                    return;
                                };

                                move_item(
                                    &source,
                                    &new_pane,
                                    item_id_to_move,
                                    new_pane.read(cx).active_item_index(),
                                    true,
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                        })
                        .detach();
                    } else if let Some(project_path) = item.project_path(cx)
                        && let Some(entry_path) = project.read(cx).absolute_path(&project_path, cx)
                    {
                        add_paths_to_terminal(pane, &[entry_path], window, cx);
                    }
                }
            } else if let Some(selection) = dropped_item.downcast_ref::<DraggedSelection>() {
                let project = project.read(cx);
                let paths_to_add = selection
                    .items()
                    .map(|selected_entry| selected_entry.entry_id)
                    .filter_map(|entry_id| project.path_for_entry(entry_id, cx))
                    .filter_map(|project_path| project.absolute_path(&project_path, cx))
                    .collect::<Vec<_>>();
                if !paths_to_add.is_empty() {
                    add_paths_to_terminal(pane, &paths_to_add, window, cx);
                }
            } else if let Some(&entry_id) = dropped_item.downcast_ref::<ProjectEntryId>() {
                if let Some(entry_path) = project
                    .read(cx)
                    .path_for_entry(entry_id, cx)
                    .and_then(|project_path| project.read(cx).absolute_path(&project_path, cx))
                {
                    add_paths_to_terminal(pane, &[entry_path], window, cx);
                }
            } else if is_local && let Some(paths) = dropped_item.downcast_ref::<ExternalPaths>() {
                add_paths_to_terminal(pane, paths.paths(), window, cx);
            }

            ControlFlow::Break(())
        });

        pane
    });

    cx.subscribe_in(&pane, window, TerminalPanel::handle_pane_event)
        .detach();
    cx.observe(&pane, |_, _, cx| cx.notify()).detach();

    pane
}

async fn wait_for_terminals_tasks(
    terminals_for_task: Vec<(usize, Entity<Pane>, Entity<TerminalView>)>,
    cx: &mut AsyncApp,
) {
    let pending_tasks = terminals_for_task.iter().map(|(_, _, terminal)| {
        terminal.update(cx, |terminal_view, cx| {
            terminal_view
                .terminal()
                .update(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
        })
    });
    join_all(pending_tasks).await;
}

fn add_paths_to_terminal(
    pane: &mut Pane,
    paths: &[PathBuf],
    window: &mut Window,
    cx: &mut Context<Pane>,
) {
    if let Some(terminal_view) = pane
        .active_item()
        .and_then(|item| item.downcast::<TerminalView>())
    {
        window.focus(&terminal_view.focus_handle(cx), cx);
        let mut new_text = paths.iter().map(|path| format!(" {path:?}")).join("");
        new_text.push(' ');
        terminal_view.update(cx, |terminal_view, cx| {
            terminal_view.terminal().update(cx, |terminal, _| {
                terminal.paste(&new_text);
            });
        });
    }
}

struct FailedToSpawnTerminal {
    error: String,
    focus_handle: FocusHandle,
}

impl Focusable for FailedToSpawnTerminal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FailedToSpawnTerminal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let popover_menu = PopoverMenu::new("settings-popover")
            .trigger(
                IconButton::new("icon-button-popover", IconName::ChevronDown)
                    .icon_size(IconSize::XSmall),
            )
            .menu(move |window, cx| {
                Some(ContextMenu::build(window, cx, |context_menu, _, _| {
                    context_menu
                        .action(t("terminal_panel.open_settings"), bspterm_actions::OpenSettings.boxed_clone())
                        .action(
                            t("terminal_panel.edit_settings_json"),
                            bspterm_actions::OpenSettingsFile.boxed_clone(),
                        )
                }))
            })
            .anchor(Corner::TopRight)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(2.0),
            });

        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .p_4()
            .items_center()
            .justify_center()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .max_w_112()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(Label::new(t("terminal_panel.failed_to_spawn")))
                    .child(
                        Label::new(self.error.to_string())
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .mb_4(),
                    )
                    .child(SplitButton::new(
                        ButtonLike::new("open-settings-ui")
                            .child(Label::new(t("terminal_panel.edit_settings")).size(LabelSize::Small))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(bspterm_actions::OpenSettings.boxed_clone(), cx);
                            }),
                        popover_menu.into_any_element(),
                    )),
            )
    }
}

impl EventEmitter<()> for FailedToSpawnTerminal {}

impl workspace::Item for FailedToSpawnTerminal {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        t("terminal_panel.failed_to_spawn")
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let registrar = cx
            .try_global::<workspace::PaneSearchBarCallbacks>()
            .map(|callbacks| {
                (callbacks.wrap_div_with_search_actions)(div(), self.active_pane.clone())
            })
            .unwrap_or_else(div);
        self.workspace
            .update(cx, |workspace, cx| {
                registrar.size_full().child(self.center.render(
                    workspace.zoomed_item(),
                    &workspace::PaneRenderContext {
                        follower_states: &HashMap::default(),
                        active_call: workspace.active_call(),
                        active_pane: &self.active_pane,
                        app_state: workspace.app_state(),
                        project: workspace.project(),
                        workspace: &workspace.weak_handle(),
                    },
                    window,
                    cx,
                ))
            })
            .ok()
            .map(|div| {
                div.on_action({
                    cx.listener(|terminal_panel, _: &ActivatePaneLeft, window, cx| {
                        terminal_panel.activate_pane_in_direction(SplitDirection::Left, window, cx);
                    })
                })
                .on_action({
                    cx.listener(|terminal_panel, _: &ActivatePaneRight, window, cx| {
                        terminal_panel.activate_pane_in_direction(
                            SplitDirection::Right,
                            window,
                            cx,
                        );
                    })
                })
                .on_action({
                    cx.listener(|terminal_panel, _: &ActivatePaneUp, window, cx| {
                        terminal_panel.activate_pane_in_direction(SplitDirection::Up, window, cx);
                    })
                })
                .on_action({
                    cx.listener(|terminal_panel, _: &ActivatePaneDown, window, cx| {
                        terminal_panel.activate_pane_in_direction(SplitDirection::Down, window, cx);
                    })
                })
                .on_action(
                    cx.listener(|terminal_panel, _action: &ActivateNextPane, window, cx| {
                        let panes = terminal_panel.center.panes();
                        if let Some(ix) = panes
                            .iter()
                            .position(|pane| **pane == terminal_panel.active_pane)
                        {
                            let next_ix = (ix + 1) % panes.len();
                            window.focus(&panes[next_ix].focus_handle(cx), cx);
                        }
                    }),
                )
                .on_action(cx.listener(
                    |terminal_panel, _action: &ActivatePreviousPane, window, cx| {
                        let panes = terminal_panel.center.panes();
                        if let Some(ix) = panes
                            .iter()
                            .position(|pane| **pane == terminal_panel.active_pane)
                        {
                            let prev_ix = cmp::min(ix.wrapping_sub(1), panes.len() - 1);
                            window.focus(&panes[prev_ix].focus_handle(cx), cx);
                        }
                    },
                ))
                .on_action(
                    cx.listener(|terminal_panel, action: &ActivatePane, window, cx| {
                        let panes = terminal_panel.center.panes();
                        if let Some(&pane) = panes.get(action.0) {
                            window.focus(&pane.read(cx).focus_handle(cx), cx);
                        } else {
                            let future =
                                terminal_panel.new_pane_with_active_terminal(true, window, cx);
                            cx.spawn_in(window, async move |terminal_panel, cx| {
                                if let Some(new_pane) = future.await {
                                    _ = terminal_panel.update_in(
                                        cx,
                                        |terminal_panel, window, cx| {
                                            terminal_panel
                                                .center
                                                .split(
                                                    &terminal_panel.active_pane,
                                                    &new_pane,
                                                    SplitDirection::Right,
                                                    cx,
                                                )
                                                .log_err();
                                            let new_pane = new_pane.read(cx);
                                            window.focus(&new_pane.focus_handle(cx), cx);
                                        },
                                    );
                                }
                            })
                            .detach();
                        }
                    }),
                )
                .on_action(cx.listener(|terminal_panel, _: &SwapPaneLeft, _, cx| {
                    terminal_panel.swap_pane_in_direction(SplitDirection::Left, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &SwapPaneRight, _, cx| {
                    terminal_panel.swap_pane_in_direction(SplitDirection::Right, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &SwapPaneUp, _, cx| {
                    terminal_panel.swap_pane_in_direction(SplitDirection::Up, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &SwapPaneDown, _, cx| {
                    terminal_panel.swap_pane_in_direction(SplitDirection::Down, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &MovePaneLeft, _, cx| {
                    terminal_panel.move_pane_to_border(SplitDirection::Left, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &MovePaneRight, _, cx| {
                    terminal_panel.move_pane_to_border(SplitDirection::Right, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &MovePaneUp, _, cx| {
                    terminal_panel.move_pane_to_border(SplitDirection::Up, cx);
                }))
                .on_action(cx.listener(|terminal_panel, _: &MovePaneDown, _, cx| {
                    terminal_panel.move_pane_to_border(SplitDirection::Down, cx);
                }))
                .on_action(
                    cx.listener(|terminal_panel, action: &MoveItemToPane, window, cx| {
                        let Some(&target_pane) =
                            terminal_panel.center.panes().get(action.destination)
                        else {
                            return;
                        };
                        move_active_item(
                            &terminal_panel.active_pane,
                            target_pane,
                            action.focus,
                            true,
                            window,
                            cx,
                        );
                    }),
                )
                .on_action(cx.listener(
                    |terminal_panel, action: &MoveItemToPaneInDirection, window, cx| {
                        let source_pane = &terminal_panel.active_pane;
                        if let Some(destination_pane) = terminal_panel
                            .center
                            .find_pane_in_direction(source_pane, action.direction, cx)
                        {
                            move_active_item(
                                source_pane,
                                destination_pane,
                                action.focus,
                                true,
                                window,
                                cx,
                            );
                        };
                    },
                ))
            })
            .unwrap_or_else(|| div())
    }
}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.active_pane.focus_handle(cx)
    }
}

impl Panel for TerminalPanel {
    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        match TerminalSettings::get_global(cx).dock {
            TerminalDockPosition::Left => DockPosition::Left,
            TerminalDockPosition::Bottom => DockPosition::Bottom,
            TerminalDockPosition::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, _: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            let dock = match position {
                DockPosition::Left => TerminalDockPosition::Left,
                DockPosition::Bottom => TerminalDockPosition::Bottom,
                DockPosition::Right => TerminalDockPosition::Right,
            };
            settings.terminal.get_or_insert_default().dock = Some(dock);
        });
    }

    fn size(&self, window: &Window, cx: &App) -> Pixels {
        let settings = TerminalSettings::get_global(cx);
        match self.position(window, cx) {
            DockPosition::Left | DockPosition::Right => {
                self.width.unwrap_or(settings.default_width)
            }
            DockPosition::Bottom => self.height.unwrap_or(settings.default_height),
        }
    }

    fn set_size(&mut self, size: Option<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        match self.position(window, cx) {
            DockPosition::Left | DockPosition::Right => self.width = size,
            DockPosition::Bottom => self.height = size,
        }
        cx.notify();
        cx.defer_in(window, |this, _, cx| {
            this.serialize(cx);
        })
    }

    fn is_zoomed(&self, _window: &Window, cx: &App) -> bool {
        self.active_pane.read(cx).is_zoomed()
    }

    fn set_zoomed(&mut self, zoomed: bool, _: &mut Window, cx: &mut Context<Self>) {
        for pane in self.center.panes() {
            pane.update(cx, |pane, cx| {
                pane.set_zoomed(zoomed, cx);
            })
        }
        cx.notify();
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        let old_active = self.active;
        self.active = active;
        if !active || old_active == active || !self.has_no_terminals(cx) {
            return;
        }
        cx.defer_in(window, |this, window, cx| {
            let Ok(kind) = this
                .workspace
                .update(cx, |workspace, cx| default_working_directory(workspace, cx))
            else {
                return;
            };

            this.add_terminal_shell(kind, RevealStrategy::Always, window, cx)
                .detach_and_log_err(cx)
        })
    }

    fn icon_label(&self, _window: &Window, cx: &App) -> Option<String> {
        let count = self
            .center
            .panes()
            .into_iter()
            .map(|pane| pane.read(cx).items_len())
            .sum::<usize>();
        if count == 0 {
            None
        } else {
            Some(count.to_string())
        }
    }

    fn persistent_name() -> &'static str {
        "TerminalPanel"
    }

    fn panel_key() -> &'static str {
        TERMINAL_PANEL_KEY
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        if (self.is_enabled(cx) || !self.has_no_terminals(cx))
            && TerminalSettings::get_global(cx).button
        {
            Some(IconName::TerminalAlt)
        } else {
            None
        }
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("terminal_panel.title")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(ToggleFocus)
    }

    fn pane(&self) -> Option<Entity<Pane>> {
        Some(self.active_pane.clone())
    }

    fn activation_priority(&self) -> u32 {
        1
    }
}

struct TerminalProvider(Entity<TerminalPanel>);

impl workspace::TerminalProvider for TerminalProvider {
    fn spawn(
        &self,
        task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Option<Result<ExitStatus>>> {
        let terminal_panel = self.0.clone();
        window.spawn(cx, async move |cx| {
            let terminal = terminal_panel
                .update_in(cx, |terminal_panel, window, cx| {
                    terminal_panel.spawn_task(&task, window, cx)
                })
                .ok()?
                .await;
            match terminal {
                Ok(terminal) => {
                    let exit_status = terminal
                        .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
                        .ok()?
                        .await?;
                    Some(Ok(exit_status))
                }
                Err(e) => Some(Err(e)),
            }
        })
    }
}

struct InlineAssistTabBarButton {
    focus_handle: FocusHandle,
}

impl Render for InlineAssistTabBarButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        IconButton::new("terminal_inline_assistant", IconName::ZedAssistant)
            .icon_size(IconSize::Small)
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(InlineAssist::default().boxed_clone(), cx);
            }))
            .tooltip(move |_window, cx| {
                Tooltip::for_action_in("Inline Assist", &InlineAssist::default(), &focus_handle, cx)
            })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use super::*;
    use gpui::{TestAppContext, UpdateGlobal as _};
    use pretty_assertions::assert_eq;
    use project::FakeFs;
    use settings::SettingsStore;

    #[test]
    fn test_prepare_empty_task() {
        let input = SpawnInTerminal::default();
        let shell = Shell::System;

        let result = prepare_task_for_spawn(&input, &shell, false);

        let expected_shell = util::get_system_shell();
        assert_eq!(result.env, HashMap::default());
        assert_eq!(result.cwd, None);
        assert_eq!(result.shell, Shell::System);
        assert_eq!(
            result.command,
            Some(expected_shell.clone()),
            "Empty tasks should spawn a -i shell"
        );
        assert_eq!(result.args, Vec::<String>::new());
        assert_eq!(
            result.command_label, expected_shell,
            "We show the shell launch for empty commands"
        );
    }

    #[gpui::test]
    async fn test_bypass_max_tabs_limit(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let workspace = cx.add_window(|window, cx| Workspace::test_new(project, window, cx));

        let (window_handle, terminal_panel) = workspace
            .update(cx, |workspace, window, cx| {
                let window_handle = window.window_handle();
                let terminal_panel = cx.new(|cx| TerminalPanel::new(workspace, window, cx));
                (window_handle, terminal_panel)
            })
            .unwrap();

        set_max_tabs(cx, Some(3));

        for _ in 0..5 {
            let task = window_handle
                .update(cx, |_, window, cx| {
                    terminal_panel.update(cx, |panel, cx| {
                        panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
                    })
                })
                .unwrap();
            task.await.unwrap();
        }

        cx.run_until_parked();

        let item_count =
            terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

        assert_eq!(
            item_count, 5,
            "Terminal panel should bypass max_tabs limit and have all 5 terminals"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_prepare_script_like_task() {
        let user_command = r#"REPO_URL=$(git remote get-url origin | sed -e \"s/^git@\\(.*\\):\\(.*\\)\\.git$/https:\\/\\/\\1\\/\\2/\"); COMMIT_SHA=$(git log -1 --format=\"%H\" -- \"${ZED_RELATIVE_FILE}\"); echo \"${REPO_URL}/blob/${COMMIT_SHA}/${ZED_RELATIVE_FILE}#L${ZED_ROW}-$(echo $(($(wc -l <<< \"$ZED_SELECTED_TEXT\") + $ZED_ROW - 1)))\" | xclip -selection clipboard"#.to_string();
        let expected_cwd = PathBuf::from("/some/work");

        let input = SpawnInTerminal {
            command: Some(user_command.clone()),
            cwd: Some(expected_cwd.clone()),
            ..SpawnInTerminal::default()
        };
        let shell = Shell::System;

        let result = prepare_task_for_spawn(&input, &shell, false);

        let system_shell = util::get_system_shell();
        assert_eq!(result.env, HashMap::default());
        assert_eq!(result.cwd, Some(expected_cwd));
        assert_eq!(result.shell, Shell::System);
        assert_eq!(result.command, Some(system_shell.clone()));
        assert_eq!(
            result.args,
            vec!["-i".to_string(), "-c".to_string(), user_command.clone()],
            "User command should have been moved into the arguments, as we're spawning a new -i shell",
        );
        assert_eq!(
            result.command_label,
            format!(
                "{system_shell} {interactive}-c '{user_command}'",
                interactive = if cfg!(windows) { "" } else { "-i " }
            ),
            "We want to show to the user the entire command spawned"
        );
    }

    #[gpui::test]
    async fn renders_error_if_default_shell_fails(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.terminal.get_or_insert_default().project.shell =
                        Some(settings::Shell::Program("__nonexistent_shell__".to_owned()));
                });
            });
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let workspace = cx.add_window(|window, cx| Workspace::test_new(project, window, cx));

        let (window_handle, terminal_panel) = workspace
            .update(cx, |workspace, window, cx| {
                let window_handle = window.window_handle();
                let terminal_panel = cx.new(|cx| TerminalPanel::new(workspace, window, cx));
                (window_handle, terminal_panel)
            })
            .unwrap();

        window_handle
            .update(cx, |_, window, cx| {
                terminal_panel.update(cx, |terminal_panel, cx| {
                    terminal_panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
                })
            })
            .unwrap()
            .await
            .unwrap_err();

        window_handle
            .update(cx, |_, _, cx| {
                terminal_panel.update(cx, |terminal_panel, cx| {
                    assert!(
                        terminal_panel
                            .active_pane
                            .read(cx)
                            .items()
                            .any(|item| item.downcast::<FailedToSpawnTerminal>().is_some()),
                        "should spawn `FailedToSpawnTerminal` pane"
                    );
                })
            })
            .unwrap();
    }

    #[gpui::test]
    async fn test_local_terminal_in_local_project(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let workspace = cx.add_window(|window, cx| Workspace::test_new(project, window, cx));

        let (window_handle, terminal_panel) = workspace
            .update(cx, |workspace, window, cx| {
                let window_handle = window.window_handle();
                let terminal_panel = cx.new(|cx| TerminalPanel::new(workspace, window, cx));
                (window_handle, terminal_panel)
            })
            .unwrap();

        let result = window_handle
            .update(cx, |_, window, cx| {
                terminal_panel.update(cx, |terminal_panel, cx| {
                    terminal_panel.add_local_terminal_shell(RevealStrategy::Always, window, cx)
                })
            })
            .unwrap()
            .await;

        assert!(
            result.is_ok(),
            "local terminal should successfully create in local project"
        );
    }

    fn set_max_tabs(cx: &mut TestAppContext, value: Option<usize>) {
        cx.update_global(|store: &mut SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings.workspace.max_tabs = value.map(|v| NonZero::new(v).unwrap())
            });
        });
    }

    pub fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let store = SettingsStore::test(cx);
            cx.set_global(store);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::init(cx);
        });
    }
}
