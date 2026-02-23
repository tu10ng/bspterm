use anyhow::Result;
use bspterm_actions::command_panel::{Clear, Send, ToggleFocus};
use collections::HashMap;
use editor::{Editor, EditorMode, MultiBuffer, ToPoint};
use gpui::{
    Action, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, WeakEntity, Window, px,
};
use i18n::t;
use language::Buffer;
use multi_buffer::MultiBufferRow;
use terminal::Terminal;
use terminal_view::TerminalView;
use text::Point;
use ui::{
    prelude::*, Color, Icon, IconName, IconSize, Label, LabelSize, Tooltip,
    h_flex, v_flex,
};
use workspace::{
    Workspace, Event as WorkspaceEvent,
    dock::{DockPosition, Panel, PanelEvent},
};

const COMMAND_PANEL_KEY: &str = "CommandPanel";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<CommandPanel>(window, cx);
        });
    })
    .detach();
}

pub struct CommandPanel {
    editor: Entity<Editor>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    terminal_buffers: HashMap<EntityId, String>,
    current_terminal: Option<EntityId>,
    _subscriptions: Vec<Subscription>,
}

impl CommandPanel {
    fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let buffer = cx.new(|cx| Buffer::local("", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

        let editor = cx.new(|cx| {
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
        });

        let subscriptions = if let Some(ws) = workspace.upgrade() {
            vec![cx.subscribe_in(&ws, window, Self::handle_workspace_event)]
        } else {
            vec![]
        };

        Self {
            editor,
            workspace,
            focus_handle,
            width: None,
            terminal_buffers: HashMap::default(),
            current_terminal: None,
            _subscriptions: subscriptions,
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: gpui::AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        cx.update(|window, cx| cx.new(|cx| Self::new(workspace, window, cx)))
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

        log::debug!(
            "[CommandPanel] on_terminal_changed: current={:?}, new={:?}",
            self.current_terminal,
            new_id
        );

        if new_id != self.current_terminal {
            if let Some(old_id) = self.current_terminal {
                let text = self.editor.read(cx).text(cx);
                log::debug!(
                    "[CommandPanel] Saving buffer for terminal {:?}: {:?}",
                    old_id,
                    text
                );
                self.terminal_buffers.insert(old_id, text);
            }

            let content = new_id
                .and_then(|id| self.terminal_buffers.get(&id))
                .cloned()
                .unwrap_or_default();

            log::debug!(
                "[CommandPanel] Loading buffer for terminal {:?}: {:?}",
                new_id,
                content
            );

            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, window, cx);
            });

            self.current_terminal = new_id;
        }
    }

    fn send_to_terminal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // Get cursor position using anchor and convert to point
        let cursor_anchor = editor.selections.newest_anchor().head();
        let cursor = cursor_anchor.to_point(&snapshot);
        let row = cursor.row;

        // Get current line text
        let line_len = snapshot.line_len(MultiBufferRow(row));
        let line_text: String = snapshot
            .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
            .collect();

        if line_text.is_empty() {
            return;
        }

        if let Some(terminal) = self.get_focused_terminal(cx) {
            terminal.update(cx, |terminal, _cx| {
                // Send line with newline
                let mut text = line_text;
                text.push('\n');
                terminal.input(text.into_bytes());
            });
        } else {
            log::warn!("{}", t("command_panel.no_terminal"));
        }
        // Note: removed self.clear_editor() - keep buffer contents
    }

    fn clear_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor.delete(&editor::actions::Delete, window, cx);
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
            .border_color(colors.border)
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

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .justify_end()
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
            )
    }
}

impl EventEmitter<PanelEvent> for CommandPanel {}

impl Render for CommandPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let panel_bg = colors.panel_background;

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
            .child(self.render_hint_bar(cx))
            .child(self.render_toolbar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(self.editor.clone()),
            )
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
