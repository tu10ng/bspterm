use anyhow::Result;
use bspterm_actions::command_panel::{
    AddTab, Clear, CloseTab, RenameTab, Send, StartCycleSend, StopCycleSend, ToggleFocus,
};
use collections::HashMap;
use editor::{Editor, EditorEvent, EditorMode, HighlightKey, MultiBuffer, SizingBehavior, ToPoint};
use gpui::{
    Action, App, ClickEvent, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    FontStyle, HighlightStyle, IntoElement, MouseButton, Pixels, Render, SharedString, Styled,
    Subscription, Task, WeakEntity, Window, px,
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

#[derive(Clone)]
struct DraggedCommandTab {
    index: usize,
    label: SharedString,
}

impl Render for DraggedCommandTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(cx.theme().colors().element_active)
            .rounded_md()
            .shadow_md()
            .opacity(0.8)
            .child(Label::new(self.label.clone()).size(LabelSize::Small))
    }
}

const COMMAND_PANEL_KEY: &str = "CommandPanel";
const DEFAULT_CYCLE_INTERVAL_MS: u64 = 5000;
const MIN_CYCLE_INTERVAL_MS: u64 = 500;
const CYCLE_INTERVAL_STEP_MS: u64 = 500;

#[derive(Clone, Default, Serialize, Deserialize)]
struct PerTerminalPersistConfig {
    session_id: String,
    #[serde(default)]
    command_content: String,
    #[serde(default)]
    cycle_content: String,
    #[serde(default)]
    cycle_interval_ms: u64,
}

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
    #[serde(default)]
    cycle_interval_ms: u64,
    #[serde(default)]
    cycle_content: String,
    #[serde(default)]
    per_terminal: Vec<PerTerminalPersistConfig>,
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
    // Per-terminal cycle intervals (keyed by Terminal EntityId)
    cycle_intervals: HashMap<EntityId, u64>,
    // Per-terminal running cycle tasks (keyed by Terminal EntityId)
    cycle_tasks: HashMap<EntityId, Task<Result<()>>>,
    // TerminalView EntityId -> Terminal EntityId (for cleanup on tab close)
    view_to_terminal: HashMap<EntityId, EntityId>,
    // Terminal EntityId -> session_id (for persistence)
    terminal_session_ids: HashMap<EntityId, Uuid>,
    // Loaded-but-unmatched persisted per-terminal configs (keyed by session_id)
    pending_session_states: HashMap<Uuid, PerTerminalPersistConfig>,
    renaming_tab: Option<usize>,
    rename_editor: Option<Entity<Editor>>,
    rename_focus_subscription: Option<Subscription>,
    save_task: Option<Task<()>>,
    initialized: bool,
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
            terminal_buffers: Some(HashMap::default()),
        };

        let mut tabs = vec![terminal_tab, cycle_tab];
        let mut pending_session_states: HashMap<Uuid, PerTerminalPersistConfig> = HashMap::default();

        if let Some(config) = Self::load_tabs_config() {
            // Load per-terminal configs into pending states
            for per_term in config.per_terminal {
                if let Ok(session_id) = Uuid::parse_str(&per_term.session_id) {
                    pending_session_states.insert(session_id, per_term);
                }
            }
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

        let mut subscriptions = if let Some(ws) = workspace.upgrade() {
            vec![cx.subscribe_in(&ws, window, Self::handle_workspace_event)]
        } else {
            vec![]
        };

        // Subscribe to editor changes for debounced save and comment highlights (skip terminal-specific tab at index 0)
        for tab in tabs.iter().skip(1) {
            let editor = &tab.editor;
            subscriptions.push(cx.subscribe(editor, |this, editor, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.schedule_debounced_save(cx);
                    this.update_comment_highlights(&editor, cx);
                }
            }));
        }

        // Subscribe to terminal-specific tab (index 0) for comment highlights
        subscriptions.push(cx.subscribe(&tabs[0].editor, |this, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                this.update_comment_highlights(&editor, cx);
            }
        }));

        // Apply comment highlights for tabs with initial content
        cx.defer_in(window, |this, window, cx| {
            this.on_terminal_changed(window, cx);
            this.initialized = true;
            for tab in &this.tabs {
                this.update_comment_highlights(&tab.editor.clone(), cx);
            }
        });

        Self {
            tabs,
            active_tab_index: 0,
            workspace,
            focus_handle,
            width: None,
            current_terminal: None,
            cycle_intervals: HashMap::default(),
            cycle_tasks: HashMap::default(),
            view_to_terminal: HashMap::default(),
            terminal_session_ids: HashMap::default(),
            pending_session_states,
            renaming_tab: None,
            rename_editor: None,
            rename_focus_subscription: None,
            save_task: None,
            initialized: false,
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
                EditorMode::Full {
                    scale_ui_elements_with_buffer_font_size: true,
                    show_active_line_background: true,
                    sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
                },
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

    fn get_focused_terminal_view(&self, cx: &App) -> Option<Entity<TerminalView>> {
        let workspace = self.workspace.upgrade()?;
        let workspace = workspace.read(cx);
        let active_pane = workspace.active_pane().read(cx);
        let active_item = active_pane.active_item()?;
        active_item.downcast::<TerminalView>()
    }

    fn handle_workspace_event(
        &mut self,
        _workspace: &Entity<Workspace>,
        event: &WorkspaceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkspaceEvent::ActiveItemChanged => {
                self.on_terminal_changed(window, cx);
            }
            WorkspaceEvent::ItemRemoved { item_id } => {
                if let Some(terminal_id) = self.view_to_terminal.remove(item_id) {
                    self.cleanup_terminal(terminal_id, cx);
                }
            }
            _ => {}
        }
    }

    fn cleanup_terminal(&mut self, terminal_id: EntityId, cx: &mut Context<Self>) {
        if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
            terminal_buffers.remove(&terminal_id);
        }
        if let Some(terminal_buffers) = &mut self.tabs[1].terminal_buffers {
            terminal_buffers.remove(&terminal_id);
        }
        self.cycle_intervals.remove(&terminal_id);
        self.cycle_tasks.remove(&terminal_id);
        self.terminal_session_ids.remove(&terminal_id);
        if self.current_terminal == Some(terminal_id) {
            self.current_terminal = None;
        }
        self.schedule_save(cx);
    }

    fn on_terminal_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_terminal_view = self.get_focused_terminal_view(cx);
        let new_terminal = new_terminal_view.as_ref().map(|tv| tv.read(cx).terminal().clone());
        let new_id = new_terminal.as_ref().map(|t| t.entity_id());

        // Track view_to_terminal mapping
        if let (Some(tv), Some(terminal)) = (&new_terminal_view, &new_terminal) {
            self.view_to_terminal.insert(tv.entity_id(), terminal.entity_id());
        }

        // Track terminal_session_ids mapping
        if let Some(terminal) = &new_terminal {
            let terminal_id = terminal.entity_id();
            if !self.terminal_session_ids.contains_key(&terminal_id) {
                let session_id = terminal.read(cx).connection_info().and_then(|ci| ci.session_id());
                if let Some(session_id) = session_id {
                    self.terminal_session_ids.insert(terminal_id, session_id);
                }
            }
        }

        if new_id != self.current_terminal {
            // Save old terminal's state — only save the active tab's editor content,
            // because the non-active tab's editor holds stale content from the last displayed terminal.
            if let Some(old_id) = self.current_terminal {
                match self.active_tab_index {
                    0 => {
                        let text = self.tabs[0].editor.read(cx).text(cx);
                        if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                            terminal_buffers.insert(old_id, text);
                        }
                    }
                    1 => {
                        let text = self.tabs[1].editor.read(cx).text(cx);
                        if let Some(terminal_buffers) = &mut self.tabs[1].terminal_buffers {
                            terminal_buffers.insert(old_id, text);
                        }
                    }
                    _ => {}
                }
            }

            // Check pending session states for new terminal
            if let Some(terminal_id) = new_id {
                let session_id = self.terminal_session_ids.get(&terminal_id).copied();
                if let Some(session_id) = session_id {
                    if let Some(pending) = self.pending_session_states.remove(&session_id) {
                        // Apply pending state to terminal_buffers
                        if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                            if !pending.command_content.is_empty() {
                                terminal_buffers.insert(terminal_id, pending.command_content);
                            }
                        }
                        if let Some(terminal_buffers) = &mut self.tabs[1].terminal_buffers {
                            if !pending.cycle_content.is_empty() {
                                terminal_buffers.insert(terminal_id, pending.cycle_content);
                            }
                        }
                        if pending.cycle_interval_ms > 0 {
                            self.cycle_intervals.insert(terminal_id, pending.cycle_interval_ms);
                        }
                    }
                }
            }

            // Restore content for the active tab
            if self.active_tab().kind == CommandTabKind::TerminalSpecific {
                let content = new_id
                    .and_then(|id| self.tabs[0].terminal_buffers.as_ref()?.get(&id))
                    .cloned()
                    .unwrap_or_default();
                self.tabs[0].editor.update(cx, |editor, cx| {
                    editor.set_text(content, window, cx);
                });
                let editor = self.tabs[0].editor.clone();
                self.update_comment_highlights(&editor, cx);
            } else if self.active_tab().kind == CommandTabKind::CycleSend {
                let content = new_id
                    .and_then(|id| self.tabs[1].terminal_buffers.as_ref()?.get(&id))
                    .cloned()
                    .unwrap_or_default();
                self.tabs[1].editor.update(cx, |editor, cx| {
                    editor.set_text(content, window, cx);
                });
                let editor = self.tabs[1].editor.clone();
                self.update_comment_highlights(&editor, cx);
            }

            self.current_terminal = new_id;
            cx.notify();
        }
    }

    fn switch_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        // Save current tab content before switching
        if let Some(current_id) = self.current_terminal {
            match self.active_tab().kind {
                CommandTabKind::TerminalSpecific => {
                    let text = self.tabs[0].editor.read(cx).text(cx);
                    if let Some(terminal_buffers) = &mut self.tabs[0].terminal_buffers {
                        terminal_buffers.insert(current_id, text);
                    }
                }
                CommandTabKind::CycleSend => {
                    let text = self.tabs[1].editor.read(cx).text(cx);
                    if let Some(terminal_buffers) = &mut self.tabs[1].terminal_buffers {
                        terminal_buffers.insert(current_id, text);
                    }
                }
                _ => {}
            }
        }

        self.active_tab_index = index;

        // Restore content for target tab
        match self.active_tab().kind {
            CommandTabKind::TerminalSpecific => {
                let content = self.current_terminal
                    .and_then(|id| self.tabs[0].terminal_buffers.as_ref()?.get(&id))
                    .cloned()
                    .unwrap_or_default();
                self.tabs[0].editor.update(cx, |editor, cx| {
                    editor.set_text(content, window, cx);
                });
                let editor = self.tabs[0].editor.clone();
                self.update_comment_highlights(&editor, cx);
            }
            CommandTabKind::CycleSend => {
                let content = self.current_terminal
                    .and_then(|id| self.tabs[1].terminal_buffers.as_ref()?.get(&id))
                    .cloned()
                    .unwrap_or_default();
                self.tabs[1].editor.update(cx, |editor, cx| {
                    editor.set_text(content, window, cx);
                });
                let editor = self.tabs[1].editor.clone();
                self.update_comment_highlights(&editor, cx);
            }
            _ => {}
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
            if !line_text.is_empty() && !line_text.starts_with('#') {
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

        if self.active_tab_index == 1 {
            // Cycle Send tab: Ctrl+Enter toggles cycle send of current/selected lines
            if self.is_current_cycle_running() {
                self.stop_cycle_send(_window, cx);
            } else {
                self.start_cycle_send_selected(cx);
            }
        } else {
            let editor = self.active_editor().clone();
            Self::send_from_editor(&editor, &terminal, cx);
        }
        self.schedule_save(cx);
    }

    fn start_cycle_send_selected(&mut self, cx: &mut Context<Self>) {
        let Some(terminal) = self.get_focused_terminal(cx) else { return };
        let terminal_id = terminal.entity_id();
        if self.cycle_tasks.contains_key(&terminal_id) { return; }

        // Extract current line or selected lines from the cycle editor
        let cycle_editor = &self.tabs[1].editor;
        let editor_read = cycle_editor.read(cx);
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
            if !line_text.is_empty() && !line_text.starts_with('#') {
                lines.push(line_text);
            }
        }

        if lines.is_empty() {
            return;
        }

        let weak_terminal = terminal.downgrade();
        let interval_ms = self.current_cycle_interval();

        let task = cx.spawn(async move |_this, cx| {
            loop {
                let terminal = match cx.update(|_cx| weak_terminal.upgrade()) {
                    Some(terminal) => terminal,
                    None => anyhow::bail!("terminal dropped"),
                };
                for line in &lines {
                    let mut text = line.clone();
                    text.push('\n');
                    cx.update(|cx| {
                        terminal.update(cx, |terminal, _cx| {
                            terminal.input(text.into_bytes());
                        });
                    });
                }

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(interval_ms))
                    .await;
            }
        });

        self.cycle_tasks.insert(terminal_id, task);
        cx.notify();
    }

    fn clear_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.active_editor().clone();
        editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor.delete(&editor::actions::Delete, window, cx);
        });
        self.schedule_save(cx);
    }

    fn start_cycle_send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(terminal) = self.get_focused_terminal(cx) else { return };
        let terminal_id = terminal.entity_id();
        if self.cycle_tasks.contains_key(&terminal_id) { return; }

        // Capture all non-comment lines from the cycle editor at start time
        let cycle_editor = self.tabs[1].editor.clone();
        let lines = {
            let editor_read = cycle_editor.read(cx);
            let snapshot = editor_read.buffer().read(cx).snapshot(cx);
            let row_count = snapshot.max_point().row;
            let mut lines = Vec::new();
            for row in 0..=row_count {
                let line_len = snapshot.line_len(MultiBufferRow(row));
                let line_text: String = snapshot
                    .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
                    .collect();
                if !line_text.is_empty() && !line_text.starts_with('#') {
                    lines.push(line_text);
                }
            }
            lines
        };

        if lines.is_empty() {
            return;
        }

        let weak_terminal = terminal.downgrade();
        let interval_ms = self.current_cycle_interval();

        let task = cx.spawn(async move |_this, cx| {
            loop {
                let terminal = match cx.update(|_cx| weak_terminal.upgrade()) {
                    Some(terminal) => terminal,
                    None => anyhow::bail!("terminal dropped"),
                };
                for line in &lines {
                    let mut text = line.clone();
                    text.push('\n');
                    cx.update(|cx| {
                        terminal.update(cx, |terminal, _cx| {
                            terminal.input(text.into_bytes());
                        });
                    });
                }

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(interval_ms))
                    .await;
            }
        });

        self.cycle_tasks.insert(terminal_id, task);
        cx.notify();
    }

    fn stop_cycle_send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal_id) = self.current_terminal {
            self.cycle_tasks.remove(&terminal_id);
        }
        cx.notify();
    }

    fn is_current_cycle_running(&self) -> bool {
        self.current_terminal
            .map(|id| self.cycle_tasks.contains_key(&id))
            .unwrap_or(false)
    }

    fn current_cycle_interval(&self) -> u64 {
        self.current_terminal
            .and_then(|id| self.cycle_intervals.get(&id))
            .copied()
            .unwrap_or(DEFAULT_CYCLE_INTERVAL_MS)
    }

    fn decrease_interval(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current_terminal {
            let entry = self.cycle_intervals.entry(id).or_insert(DEFAULT_CYCLE_INTERVAL_MS);
            *entry = (*entry).saturating_sub(CYCLE_INTERVAL_STEP_MS).max(MIN_CYCLE_INTERVAL_MS);
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn increase_interval(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current_terminal {
            let entry = self.cycle_intervals.entry(id).or_insert(DEFAULT_CYCLE_INTERVAL_MS);
            *entry = (*entry).saturating_add(CYCLE_INTERVAL_STEP_MS);
            self.schedule_save(cx);
            cx.notify();
        }
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

        self._subscriptions.push(cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                this.schedule_debounced_save(cx);
                this.update_comment_highlights(&editor, cx);
            }
        }));

        self.tabs.push(CommandTab {
            kind: CommandTabKind::UserTab { id, name },
            editor,
            terminal_buffers: None,
        });

        let new_index = self.tabs.len() - 1;
        self.switch_to_tab(new_index, window, cx);
        // Delay rename to next frame so the tab's focus handle is in the dispatch tree,
        // otherwise on_focus_out won't fire when the user clicks away.
        let entity = cx.entity();
        window.on_next_frame(move |window, cx| {
            entity.update(cx, |this, cx| {
                this.start_rename_tab(new_index, window, cx);
            });
        });
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

    fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from < 2 || to < 2 || from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        if !matches!(self.tabs[from].kind, CommandTabKind::UserTab { .. })
            || !matches!(self.tabs[to].kind, CommandTabKind::UserTab { .. })
        {
            return;
        }

        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);

        if self.active_tab_index == from {
            self.active_tab_index = to;
        } else if from < self.active_tab_index && to >= self.active_tab_index {
            self.active_tab_index -= 1;
        } else if from > self.active_tab_index && to <= self.active_tab_index {
            self.active_tab_index += 1;
        }

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

        let rename_focus_handle = rename_editor.focus_handle(cx);
        let focus_subscription = cx.on_focus_out(&rename_focus_handle, window, |this, _, window, cx| {
            this.commit_rename(window, cx);
        });

        rename_focus_handle.focus(window, cx);
        self.renaming_tab = Some(index);
        self.rename_editor = Some(rename_editor);
        self.rename_focus_subscription = Some(focus_subscription);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rename_focus_subscription = None;
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
        self.rename_focus_subscription = None;
        self.renaming_tab = None;
        self.rename_editor = None;
        self.tabs[self.active_tab_index].editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn load_tabs_config() -> Option<CommandTabsConfig> {
        let path = paths::command_tabs_file();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(error) => {
                log::warn!("[CommandPanel::load_tabs_config] failed to read file: {}", error);
                return None;
            }
        };
        match serde_json::from_str(&content) {
            Ok(config) => Some(config),
            Err(error) => {
                log::warn!("[CommandPanel::load_tabs_config] failed to parse JSON: {}", error);
                None
            }
        }
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

        // Build per-terminal persistence configs
        let mut per_terminal = Vec::new();

        // Collect all terminal EntityIds that have a session_id
        let mut terminal_ids: Vec<EntityId> = self.terminal_session_ids.keys().copied().collect();
        terminal_ids.sort();
        terminal_ids.dedup();

        for terminal_id in &terminal_ids {
            let Some(session_id) = self.terminal_session_ids.get(terminal_id) else { continue };

            // Only use editor content when this is the current terminal AND that tab is active;
            // otherwise the editor holds stale content from a different terminal.
            let command_content = if self.current_terminal == Some(*terminal_id) && self.active_tab_index == 0 {
                self.tabs[0].editor.read(cx).text(cx)
            } else {
                self.tabs[0].terminal_buffers.as_ref()
                    .and_then(|b| b.get(terminal_id))
                    .cloned()
                    .unwrap_or_default()
            };

            let cycle_content = if self.current_terminal == Some(*terminal_id) && self.active_tab_index == 1 {
                self.tabs[1].editor.read(cx).text(cx)
            } else {
                self.tabs[1].terminal_buffers.as_ref()
                    .and_then(|b| b.get(terminal_id))
                    .cloned()
                    .unwrap_or_default()
            };

            let cycle_interval_ms = self.cycle_intervals.get(terminal_id)
                .copied()
                .unwrap_or(DEFAULT_CYCLE_INTERVAL_MS);

            // Only persist if there's meaningful content
            if command_content.is_empty() && cycle_content.is_empty() && cycle_interval_ms == DEFAULT_CYCLE_INTERVAL_MS {
                continue;
            }

            per_terminal.push(PerTerminalPersistConfig {
                session_id: session_id.to_string(),
                command_content,
                cycle_content,
                cycle_interval_ms,
            });
        }

        // Preserve pending states for terminals not yet focused
        let saved_session_ids: std::collections::HashSet<String> = per_terminal
            .iter()
            .map(|p| p.session_id.clone())
            .collect();
        for (session_id, pending) in &self.pending_session_states {
            let sid = session_id.to_string();
            if !saved_session_ids.contains(&sid) {
                per_terminal.push(pending.clone());
            }
        }

        let config = CommandTabsConfig {
            tabs: user_tabs,
            cycle_interval_ms: 0,
            cycle_content: String::new(),
            per_terminal,
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

    fn schedule_debounced_save(&mut self, cx: &mut Context<Self>) {
        if !self.initialized {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            cx.update(|cx| {
                this.update(cx, |this, cx| {
                    this.schedule_save(cx);
                })
            })
            .ok();
        }));
    }

    fn update_comment_highlights(
        &self,
        editor: &Entity<Editor>,
        cx: &mut Context<Self>,
    ) {
        let editor_read = editor.read(cx);
        let snapshot = editor_read.buffer().read(cx).snapshot(cx);
        let row_count = snapshot.max_point().row;

        let mut ranges = Vec::new();
        for row in 0..=row_count {
            let line_len = snapshot.line_len(MultiBufferRow(row));
            let line_text: String = snapshot
                .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
                .collect();
            if line_text.starts_with('#') {
                let start = snapshot.anchor_before(Point::new(row, 0));
                let end = snapshot.anchor_after(Point::new(row, line_len));
                ranges.push(start..end);
            }
        }

        let comment_color = cx.theme().syntax_color("comment");
        let style = HighlightStyle {
            color: Some(comment_color),
            font_style: Some(FontStyle::Italic),
            ..Default::default()
        };

        editor.update(cx, |editor, cx| {
            if ranges.is_empty() {
                editor.clear_highlights(HighlightKey::CommentHighlight, cx);
            } else {
                editor.highlight_text(HighlightKey::CommentHighlight, ranges, style, cx);
            }
        });
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
                let drag_label = label.clone();
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
                    tab_button = tab_button
                        .on_drag(
                            DraggedCommandTab {
                                index,
                                label: drag_label,
                            },
                            |dragged, _, _, cx| {
                                cx.new(|_| DraggedCommandTab {
                                    index: dragged.index,
                                    label: dragged.label.clone(),
                                })
                            },
                        )
                        .drag_over::<DraggedCommandTab>(|style, _, _, _| {
                            style.bg(gpui::opaque_grey(0.5, 0.2))
                        })
                        .on_drop(
                            cx.listener(move |this, dragged: &DraggedCommandTab, _window, cx| {
                                this.move_tab(dragged.index, index, cx);
                            }),
                        )
                        .on_click(
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
            let cycle_running = self.is_current_cycle_running();
            let interval_text = format!("{}", self.current_cycle_interval());

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
            .on_action(cx.listener(|this, _: &RenameTab, window, cx| {
                let index = this.active_tab_index;
                if matches!(this.tabs[index].kind, CommandTabKind::UserTab { .. }) {
                    this.start_rename_tab(index, window, cx);
                }
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
        self.cycle_tasks.clear();
    }
}
