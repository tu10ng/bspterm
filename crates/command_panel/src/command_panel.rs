use anyhow::Result;
use bspterm_actions::command_panel::{
    AddTab, Clear, CloseTab, Send, StartCycleSend, StopCycleSend, ToggleFocus,
};
use collections::HashMap;
use editor::{Editor, EditorMode, MultiBuffer, ToPoint};
use gpui::{
    Action, App, ClickEvent, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    IntoElement, MouseButton, Pixels, Render, Styled, Subscription, Task, WeakEntity, Window, px,
};
use i18n::t;
use language::Buffer;
use multi_buffer::MultiBufferRow;
use serde::{Deserialize, Serialize};
use terminal::Terminal;
use terminal_view::TerminalView;
use text::Point;
use ui::{
    Color, ContextMenu, Icon, IconName, IconSize, Label, LabelSize, Tooltip, h_flex, prelude::*,
    right_click_menu, v_flex,
};
use uuid::Uuid;
use workspace::{
    Event as WorkspaceEvent, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const COMMAND_PANEL_KEY: &str = "CommandPanel";
const DEFAULT_CYCLE_INTERVAL_MS: u64 = 5000;
const MIN_CYCLE_INTERVAL_MS: u64 = 500;
const CYCLE_INTERVAL_STEP_MS: u64 = 500;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<CommandPanel>(window, cx);
        });
    })
    .detach();
}

#[derive(Clone, Debug, PartialEq)]
enum CommandTabKind {
    TerminalSpecific,
    CycleSend,
    UserTab { id: Uuid, name: String },
}

struct CommandTab {
    kind: CommandTabKind,
    editor: Entity<Editor>,
    terminal_buffers: Option<HashMap<EntityId, String>>,
}

#[derive(Serialize, Deserialize)]
struct CommandTabsConfig {
    tabs: Vec<UserTabConfig>,
    cycle_interval_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct UserTabConfig {
    id: String,
    name: String,
    content: String,
}

pub struct CommandPanel {
    tabs: Vec<CommandTab>,
    active_tab_index: usize,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    current_terminal: Option<EntityId>,
    cycle_interval_ms: u64,
    cycle_task: Option<Task<Result<()>>>,
    cycle_running: bool,
    renaming_tab: Option<usize>,
    rename_editor: Option<Entity<Editor>>,
    _subscriptions: Vec<Subscription>,
}

impl CommandPanel {
    fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let terminal_tab = CommandTab {
            kind: CommandTabKind::TerminalSpecific,
            editor: Self::create_tab_editor(window, cx),
            terminal_buffers: Some(HashMap::default()),
        };

        let cycle_tab = CommandTab {
            kind: CommandTabKind::CycleSend,
            editor: Self::create_tab_editor(window, cx),
            terminal_buffers: None,
        };

        let mut tabs = vec![terminal_tab, cycle_tab];
        let mut cycle_interval_ms = DEFAULT_CYCLE_INTERVAL_MS;

        if let Some(config) = Self::load_tabs_config() {
            cycle_interval_ms = config.cycle_interval_ms;
            for user_tab in config.tabs {
                let id = Uuid::parse_str(&user_tab.id).unwrap_or_else(|_| Uuid::new_v4());
                let editor = Self::create_tab_editor(window, cx);
                if !user_tab.content.is_empty() {
                    editor.update(cx, |editor, cx| {
                        editor.set_text(user_tab.content, window, cx);
                    });
                }
                tabs.push(CommandTab {
                    kind: CommandTabKind::UserTab {
                        id,
                        name: user_tab.name,
                    },
                    editor,
                    terminal_buffers: None,
                });
            }
        }

        let subscriptions = if let Some(ws) = workspace.upgrade() {
            vec![cx.subscribe_in(&ws, window, Self::handle_workspace_event)]
        } else {
            vec![]
        };

        Self {
            tabs,
            active_tab_index: 0,
            workspace,
            focus_handle,
            width: None,
            current_terminal: None,
            cycle_interval_ms,
            cycle_task: None,
            cycle_running: false,
            renaming_tab: None,
            rename_editor: None,
            _subscriptions: subscriptions,
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: gpui::AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        cx.update(|window, cx| cx.new(|cx| Self::new(workspace, window, cx)))
    }

    fn create_tab_editor(window: &mut Window, cx: &mut Context<Self>) -> Entity<Editor> {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

        cx.new(|cx| {
            let mut editor = Editor::new(
                EditorMode::full(),
                multi_buffer,
                None,
                window,
                cx,
            );
            editor.set_placeholder_text(&t("command_panel.placeholder"), window, cx);
            editor.set_show_gutter(false, cx);
            editor
        })
    }

    fn active_tab(&self) -> &CommandTab {
        &self.tabs[self.active_tab_index]
    }

    fn active_editor(&self) -> &Entity<Editor> {
        &self.active_tab().editor
    }

    fn get_focused_terminal(&self, cx: &App) -> Option<Entity<Terminal>> {
        let workspace = self.workspace.upgrade()?;
        let workspace = workspace.read(cx);
        let active_pane = workspace.active_pane().read(cx);
        let active_item = active_pane.active_item()?;
        let terminal_view = active_item.downcast::<TerminalView>()?;
        Some(terminal_view.read(cx).terminal().clone())
    }

    fn handle_workspace_event(
        &mut self,
        _workspace: &Entity<Workspace>,
        event: &WorkspaceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WorkspaceEvent::ActiveItemChanged = event {
            self.on_terminal_changed(window, cx);
        }
    }

    fn on_terminal_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_terminal = self.get_focused_terminal(cx);
        let new_id = new_terminal.as_ref().map(|t| t.entity_id());

        if new_id != self.current_terminal {
            // Read text before borrowing terminal_buffers to avoid borrow conflict
            let old_text = self.tabs[0].editor.read(cx).text(cx);

            if self.active_tab().kind == CommandTabKind::TerminalSpecific {
                if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                    if let Some(old_id) = self.current_terminal {
                        terminal_buffers.insert(old_id, old_text);
                    }

                    let content = new_id
                        .and_then(|id| terminal_buffers.get(&id))
                        .cloned()
                        .unwrap_or_default();

                    self.tabs[0].editor.update(cx, |editor, cx| {
                        editor.set_text(content, window, cx);
                    });
                }
            } else {
                if let Some(old_id) = self.current_terminal {
                    if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                        terminal_buffers.insert(old_id, old_text);
                    }
                }
            }

            self.current_terminal = new_id;
        }
    }

    fn switch_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        // If leaving TerminalSpecific tab, save current editor content
        if self.active_tab().kind == CommandTabKind::TerminalSpecific {
            if let Some(current_id) = self.current_terminal {
                let text = self.tabs[0].editor.read(cx).text(cx);
                if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                    terminal_buffers.insert(current_id, text);
                }
            }
        }

        self.active_tab_index = index;

        // If switching to TerminalSpecific tab, restore content for current terminal
        if self.active_tab().kind == CommandTabKind::TerminalSpecific {
            if let Some(terminal_buffers) = &self.tabs[0].terminal_buffers {
                let content = self
                    .current_terminal
                    .and_then(|id| terminal_buffers.get(&id))
                    .cloned()
                    .unwrap_or_default();
                self.tabs[0].editor.update(cx, |editor, cx| {
                    editor.set_text(content, window, cx);
                });
            }
        }

        self.tabs[self.active_tab_index].editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn send_from_editor(
        editor: &Entity<Editor>,
        terminal: &Entity<Terminal>,
        cx: &mut Context<Self>,
    ) {
        let editor_read = editor.read(cx);
        let snapshot = editor_read.buffer().read(cx).snapshot(cx);
        let selection = editor_read.selections.newest_anchor();
        let start = selection.start.to_point(&snapshot);
        let end = selection.end.to_point(&snapshot);

        let (start_row, end_row) = if start == end {
            (start.row, start.row)
        } else {
            (start.row.min(end.row), start.row.max(end.row))
        };

        let mut lines = Vec::new();
        for row in start_row..=end_row {
            let line_len = snapshot.line_len(MultiBufferRow(row));
            let line_text: String = snapshot
                .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
                .collect();
            if !line_text.is_empty() {
                lines.push(line_text);
            }
        }

        if lines.is_empty() {
            return;
        }

        let terminal = terminal.clone();
        if lines.len() == 1 {
            terminal.update(cx, |terminal, _cx| {
                let mut text = lines.into_iter().next().unwrap();
                text.push('\n');
                terminal.input(text.into_bytes());
            });
        } else {
            cx.spawn(async move |_this, cx| {
                for line in lines {
                    cx.update(|cx| {
                        terminal.update(cx, |terminal, _cx| {
                            let mut text = line;
                            text.push('\n');
                            terminal.input(text.into_bytes());
                        });
                    });
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(50))
                        .await;
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }
    }

    fn send_to_terminal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(terminal) = self.get_focused_terminal(cx) else {
            log::warn!("{}", t("command_panel.no_terminal"));
            return;
        };

        let editor = self.active_editor().clone();
        Self::send_from_editor(&editor, &terminal, cx);
    }

    fn clear_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.active_editor().clone();
        editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor.delete(&editor::actions::Delete, window, cx);
        });
    }

    fn start_cycle_send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.cycle_running {
            return;
        }

        self.cycle_running = true;
        let cycle_editor = self.tabs[1].editor.clone();
        let interval_ms = self.cycle_interval_ms;
        let workspace = self.workspace.clone();

        let task = cx.spawn(async move |_this, cx| {
            loop {
                // Get current terminal each iteration
                let terminal = cx.update(|cx| {
                    let workspace = workspace.upgrade()?;
                    let workspace = workspace.read(cx);
                    let active_pane = workspace.active_pane().read(cx);
                    let active_item = active_pane.active_item()?;
                    let terminal_view = active_item.downcast::<TerminalView>()?;
                    Some(terminal_view.read(cx).terminal().clone())
                });

                if let Some(terminal) = terminal {
                    cx.update(|cx| {
                        let editor_read = cycle_editor.read(cx);
                        let snapshot = editor_read.buffer().read(cx).snapshot(cx);
                        let row_count = snapshot.max_point().row;

                        let mut lines = Vec::new();
                        for row in 0..=row_count {
                            let line_len = snapshot.line_len(MultiBufferRow(row));
                            let line_text: String = snapshot
                                .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
                                .collect();
                            if !line_text.is_empty() {
                                lines.push(line_text);
                            }
                        }

                        if !lines.is_empty() {
                            for line in lines {
                                terminal.update(cx, |terminal, _cx| {
                                    let mut text = line;
                                    text.push('\n');
                                    terminal.input(text.into_bytes());
                                });
                            }
                        }
                    });
                }

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(interval_ms))
                    .await;
            }
        });

        self.cycle_task = Some(task);
        cx.notify();
    }

    fn stop_cycle_send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_task = None;
        self.cycle_running = false;
        cx.notify();
    }

    fn decrease_interval(&mut self, cx: &mut Context<Self>) {
        if self.cycle_interval_ms > MIN_CYCLE_INTERVAL_MS {
            self.cycle_interval_ms = self.cycle_interval_ms.saturating_sub(CYCLE_INTERVAL_STEP_MS);
            if self.cycle_interval_ms < MIN_CYCLE_INTERVAL_MS {
                self.cycle_interval_ms = MIN_CYCLE_INTERVAL_MS;
            }
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn increase_interval(&mut self, cx: &mut Context<Self>) {
        self.cycle_interval_ms = self.cycle_interval_ms.saturating_add(CYCLE_INTERVAL_STEP_MS);
        self.schedule_save(cx);
        cx.notify();
    }

    fn add_user_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let user_tab_count = self
            .tabs
            .iter()
            .filter(|tab| matches!(tab.kind, CommandTabKind::UserTab { .. }))
            .count();
        let name = format!(
            "{} {}",
            t("command_panel.default_tab_name"),
            user_tab_count + 1
        );
        let id = Uuid::new_v4();
        let editor = Self::create_tab_editor(window, cx);

        self.tabs.push(CommandTab {
            kind: CommandTabKind::UserTab { id, name },
            editor,
            terminal_buffers: None,
        });

        let new_index = self.tabs.len() - 1;
        self.switch_to_tab(new_index, window, cx);
        self.schedule_save(cx);
    }

    fn close_user_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < 2 || index >= self.tabs.len() {
            return;
        }
        if !matches!(self.tabs[index].kind, CommandTabKind::UserTab { .. }) {
            return;
        }

        self.tabs.remove(index);

        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        } else if self.active_tab_index > index {
            self.active_tab_index -= 1;
        } else if self.active_tab_index == index {
            self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
        }

        self.tabs[self.active_tab_index].editor.focus_handle(cx).focus(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn start_rename_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.tabs[index].kind, CommandTabKind::UserTab { .. }) {
            return;
        }

        let current_name = match &self.tabs[index].kind {
            CommandTabKind::UserTab { name, .. } => name.clone(),
            _ => return,
        };

        let buffer = cx.new(|cx| Buffer::local(&current_name, cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

        let rename_editor = cx.new(|cx| {
            let mut editor = Editor::new(
                EditorMode::SingleLine,
                multi_buffer,
                None,
                window,
                cx,
            );
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor
        });

        rename_editor.focus_handle(cx).focus(window, cx);
        self.renaming_tab = Some(index);
        self.rename_editor = Some(rename_editor);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.renaming_tab.take() else {
            return;
        };
        let Some(rename_editor) = self.rename_editor.take() else {
            return;
        };

        let new_name = rename_editor.read(cx).text(cx);
        let new_name = new_name.trim().to_string();

        if !new_name.is_empty() {
            if let CommandTabKind::UserTab { name, .. } = &mut self.tabs[index].kind {
                *name = new_name;
            }
        }

        self.tabs[self.active_tab_index].editor.focus_handle(cx).focus(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming_tab = None;
        self.rename_editor = None;
        self.tabs[self.active_tab_index].editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn load_tabs_config() -> Option<CommandTabsConfig> {
        let path = paths::command_tabs_file();
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn schedule_save(&self, cx: &mut Context<Self>) {
        let mut user_tabs = Vec::new();
        for tab in &self.tabs {
            if let CommandTabKind::UserTab { id, name } = &tab.kind {
                let content = tab.editor.read(cx).text(cx);
                user_tabs.push(UserTabConfig {
                    id: id.to_string(),
                    name: name.clone(),
                    content,
                });
            }
        }

        let config = CommandTabsConfig {
            tabs: user_tabs,
            cycle_interval_ms: self.cycle_interval_ms,
        };

        cx.background_spawn(async move {
            let path = paths::command_tabs_file();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                std::fs::write(path, json).ok();
            }
        })
        .detach();
    }

    fn render_hint_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(
                Icon::new(IconName::Code)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new(t("command_panel.hint"))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_bottom_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let active_index = self.active_tab_index;
        let is_cycle_tab = active_index == 1;
        let is_renaming = self.renaming_tab.is_some();

        let mut left_tabs = h_flex().gap_0();

        for (index, tab) in self.tabs.iter().enumerate() {
            let label: SharedString = match &tab.kind {
                CommandTabKind::TerminalSpecific => t("command_panel.tab_terminal"),
                CommandTabKind::CycleSend => t("command_panel.tab_cycle"),
                CommandTabKind::UserTab { name, .. } => name.clone().into(),
            };
            let is_active = index == active_index;
            let is_user_tab = matches!(tab.kind, CommandTabKind::UserTab { .. });

            // Check if this tab is being renamed
            let is_being_renamed = is_renaming && self.renaming_tab == Some(index);

            if is_being_renamed {
                if let Some(rename_editor) = &self.rename_editor {
                    left_tabs = left_tabs.child(
                        h_flex()
                            .px_2()
                            .py_1()
                            .bg(colors.element_active)
                            .border_1()
                            .border_color(colors.border_focused)
                            .rounded_t_md()
                            .child(
                                div()
                                    .w(px(80.))
                                    .child(rename_editor.clone())
                                    .on_action(cx.listener(|this, _: &editor::actions::Cancel, window, cx| {
                                        this.cancel_rename(window, cx);
                                    }))
                                    .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                        this.commit_rename(window, cx);
                                    })),
                            ),
                    );
                }
            } else {
                let mut tab_button = h_flex()
                    .id(("tab", index))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .gap_1()
                    .child(
                        Label::new(label)
                            .size(LabelSize::Small)
                            .color(if is_active {
                                Color::Default
                            } else {
                                Color::Muted
                            }),
                    );

                if is_active {
                    tab_button = tab_button
                        .bg(colors.element_active)
                        .rounded_t_md();
                }

                tab_button = tab_button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.switch_to_tab(index, window, cx);
                    }),
                );

                if is_user_tab {
                    tab_button = tab_button.on_click(
                        cx.listener(move |this, event: &ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.start_rename_tab(index, window, cx);
                            }
                        }),
                    );

                    let tab_element = tab_button.into_any_element();
                    let panel_handle = cx.weak_entity();
                    let menu = right_click_menu(("tab-menu", index))
                        .trigger(|_, _, _| tab_element)
                        .menu(move |window, cx| {
                            let rename_handle = panel_handle.clone();
                            let close_handle = panel_handle.clone();
                            ContextMenu::build(window, cx, move |menu, _window, _cx| {
                                menu.entry(
                                    t("command_panel.rename_tab"),
                                    None,
                                    move |window, cx| {
                                        if let Some(panel) = rename_handle.upgrade() {
                                            window.defer(cx, move |window, cx| {
                                                panel.update(cx, |this, cx| {
                                                    this.start_rename_tab(index, window, cx);
                                                });
                                            });
                                        }
                                    },
                                )
                                .entry(
                                    t("command_panel.close_tab"),
                                    None,
                                    move |window, cx| {
                                        if let Some(panel) = close_handle.upgrade() {
                                            window.defer(cx, move |window, cx| {
                                                panel.update(cx, |this, cx| {
                                                    this.close_user_tab(index, window, cx);
                                                });
                                            });
                                        }
                                    },
                                )
                            })
                        });
                    left_tabs = left_tabs.child(menu);
                } else {
                    left_tabs = left_tabs.child(tab_button);
                }
            }
        }

        // Add tab "+" button
        left_tabs = left_tabs.child(
            div()
                .id("add-tab")
                .px_1()
                .py_1()
                .cursor_pointer()
                .child(
                    Icon::new(IconName::Plus)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.add_user_tab(window, cx);
                    }),
                ),
        );

        let mut right_controls = h_flex().gap_1();

        // Cycle send controls (only when cycle tab active)
        if is_cycle_tab {
            let cycle_running = self.cycle_running;
            let interval_text = format!("{}", self.cycle_interval_ms);

            right_controls = right_controls
                .child(
                    Label::new(t("command_panel.cycle_interval"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    div()
                        .id("decrease-interval")
                        .px_1()
                        .cursor_pointer()
                        .child(
                            Icon::new(IconName::Dash)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _window, cx| {
                                this.decrease_interval(cx);
                            }),
                        ),
                )
                .child(
                    Label::new(interval_text)
                        .size(LabelSize::Small)
                        .color(Color::Default),
                )
                .child(
                    Label::new(t("command_panel.cycle_ms"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    div()
                        .id("increase-interval")
                        .px_1()
                        .cursor_pointer()
                        .child(
                            Icon::new(IconName::Plus)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _window, cx| {
                                this.increase_interval(cx);
                            }),
                        ),
                )
                .child(
                    if cycle_running {
                        ui::Button::new("cycle-toggle", t("command_panel.cycle_stop"))
                            .style(ui::ButtonStyle::Filled)
                            .size(ui::ButtonSize::Compact)
                            .icon(IconName::Stop)
                            .icon_size(IconSize::Small)
                            .icon_position(ui::IconPosition::Start)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.stop_cycle_send(window, cx);
                            }))
                    } else {
                        ui::Button::new("cycle-toggle", t("command_panel.cycle_start"))
                            .style(ui::ButtonStyle::Filled)
                            .size(ui::ButtonSize::Compact)
                            .icon(IconName::PlayFilled)
                            .icon_size(IconSize::Small)
                            .icon_position(ui::IconPosition::Start)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_cycle_send(window, cx);
                            }))
                    },
                );
        }

        // Send & Clear buttons (always)
        right_controls = right_controls
            .child(
                ui::Button::new("send", t("command_panel.send_tooltip"))
                    .style(ui::ButtonStyle::Filled)
                    .size(ui::ButtonSize::Compact)
                    .icon(IconName::Send)
                    .icon_size(IconSize::Small)
                    .icon_position(ui::IconPosition::Start)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action(t("command_panel.send_tooltip"), &Send, cx)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.send_to_terminal(window, cx);
                    })),
            )
            .child(
                ui::Button::new("clear", t("command_panel.clear_tooltip"))
                    .style(ui::ButtonStyle::Subtle)
                    .size(ui::ButtonSize::Compact)
                    .icon(IconName::Eraser)
                    .icon_size(IconSize::Small)
                    .icon_position(ui::IconPosition::Start)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action(t("command_panel.clear_tooltip"), &Clear, cx)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_editor(window, cx);
                    })),
            );

        h_flex()
            .w_full()
            .px_1()
            .py_px()
            .border_t_1()
            .border_color(colors.border_variant)
            .justify_between()
            .child(left_tabs)
            .child(right_controls)
    }
}

impl EventEmitter<PanelEvent> for CommandPanel {}

impl Render for CommandPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let panel_bg = colors.panel_background;
        let active_editor = self.active_editor().clone();

        v_flex()
            .key_context("CommandPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(panel_bg)
            .on_action(cx.listener(|this, _: &Send, window, cx| {
                this.send_to_terminal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Clear, window, cx| {
                this.clear_editor(window, cx);
            }))
            .on_action(cx.listener(|this, _: &AddTab, window, cx| {
                this.add_user_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let index = this.active_tab_index;
                this.close_user_tab(index, window, cx);
            }))
            .on_action(cx.listener(|this, _: &StartCycleSend, window, cx| {
                this.start_cycle_send(window, cx);
            }))
            .on_action(cx.listener(|this, _: &StopCycleSend, window, cx| {
                this.stop_cycle_send(window, cx);
            }))
            .child(self.render_hint_bar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .overflow_hidden()
                    .child(active_editor),
            )
            .child(self.render_bottom_bar(cx))
    }
}

impl Focusable for CommandPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CommandPanel {
    fn persistent_name() -> &'static str {
        "Command Panel"
    }

    fn panel_key() -> &'static str {
        COMMAND_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Bottom
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(
            position,
            DockPosition::Left | DockPosition::Right | DockPosition::Bottom
        )
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(200.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Code)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("command_panel.title")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        15
    }
}

impl Drop for CommandPanel {
    fn drop(&mut self) {
        // Stop cycle send on drop
        self.cycle_task = None;
        self.cycle_running = false;
    }
}
