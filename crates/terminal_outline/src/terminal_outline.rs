use std::ops::Range;

use anyhow::Result;
use bspterm_actions::terminal_outline::ToggleFocus;
use gpui::{
    Action, AnyElement, App, AsyncWindowContext, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, ListSizingBehavior, ParentElement, Pixels, Render,
    SharedString, Styled, Subscription, UniformListScrollHandle, WeakEntity, Window, div, px,
    uniform_list,
};
use i18n::t;
use terminal::{Event as TerminalEvent, Terminal, TerminalCommand};
use terminal_view::TerminalView;
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, prelude::*};
use workspace::{
    Event as WorkspaceEvent, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const TERMINAL_OUTLINE_PANEL_KEY: &str = "TerminalOutlinePanel";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<TerminalOutline>(window, cx);
        });
    })
    .detach();
}

/// Entry representing a command in the outline.
#[derive(Clone, Debug)]
struct OutlineEntry {
    command: TerminalCommand,
}

pub struct TerminalOutline {
    workspace: WeakEntity<Workspace>,
    active_terminal: Option<Entity<Terminal>>,
    entries: Vec<OutlineEntry>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    selected_index: Option<usize>,
    width: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl TerminalOutline {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let workspace_entity = weak_workspace.upgrade().expect("have a &Workspace");
        let workspace_subscription =
            cx.subscribe_in(&workspace_entity, window, Self::handle_workspace_event);

        let this = Self {
            workspace: weak_workspace,
            active_terminal: None,
            entries: Vec::new(),
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            selected_index: None,
            width: None,
            _subscriptions: vec![workspace_subscription],
        };

        // Defer initial terminal check to after construction completes,
        // since we're inside workspace.update_in() and can't read workspace here
        cx.defer_in(window, |this, _window, cx| {
            this.on_active_item_changed(cx);
        });

        this
    }

    fn handle_workspace_event(
        &mut self,
        _workspace: &Entity<Workspace>,
        event: &WorkspaceEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WorkspaceEvent::ActiveItemChanged = event {
            self.on_active_item_changed(cx);
        }
    }

    fn get_focused_terminal(&self, cx: &App) -> Option<Entity<Terminal>> {
        let workspace = self.workspace.upgrade()?;
        let workspace = workspace.read(cx);
        let active_pane = workspace.active_pane().read(cx);
        let active_item = active_pane.active_item()?;
        let terminal_view = active_item.downcast::<TerminalView>()?;
        Some(terminal_view.read(cx).terminal().clone())
    }

    fn on_active_item_changed(&mut self, cx: &mut Context<Self>) {
        let new_terminal = self.get_focused_terminal(cx);

        if let Some(terminal) = new_terminal {
            let should_update = match &self.active_terminal {
                Some(current) => current.entity_id() != terminal.entity_id(),
                None => true,
            };

            if should_update {
                let subscription = cx.subscribe(&terminal, |this, terminal, event, cx| {
                    if let TerminalEvent::CommandHistoryChanged = event {
                        this.update_entries_from_terminal(&terminal, cx);
                    }
                });
                self._subscriptions.push(subscription);

                self.active_terminal = Some(terminal.clone());
                self.update_entries_from_terminal(&terminal, cx);
            }
        }
    }

    fn update_entries_from_terminal(
        &mut self,
        terminal: &Entity<Terminal>,
        cx: &mut Context<Self>,
    ) {
        let commands = terminal.read(cx).command_history().commands();
        self.entries = commands
            .iter()
            .map(|cmd| OutlineEntry {
                command: cmd.clone(),
            })
            .collect();
        cx.notify();
    }

    fn scroll_to_command(&self, line: i32, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = &self.active_terminal {
            terminal.update(cx, |terminal, _cx| {
                terminal.scroll_to_line(line);
            });
            cx.notify();
        }
    }

    fn render_entry(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = &self.entries[ix];
        let is_selected = self.selected_index == Some(ix);
        let line = entry.command.line;

        let timestamp_str = entry
            .command
            .timestamp
            .map(|ts| ts.format("%H:%M:%S").to_string())
            .unwrap_or_default();

        let prompt = entry.command.prompt.clone();
        let command_text = entry.command.command_text.clone();

        div()
            .id(SharedString::from(format!("outline-entry-{}", ix)))
            .px_2()
            .py_1()
            .w_full()
            .cursor_pointer()
            .rounded_sm()
            .when(is_selected, |this| {
                this.bg(cx.theme().colors().element_selected)
            })
            .hover(|style| style.bg(cx.theme().colors().element_hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.selected_index = Some(ix);
                this.scroll_to_command(line, window, cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Label::new(timestamp_str)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(prompt)
                                    .size(LabelSize::Small)
                                    .color(Color::Accent),
                            ),
                    )
                    .child(
                        div()
                            .pl_4()
                            .child(Label::new(command_text).size(LabelSize::Small)),
                    ),
            )
            .into_any_element()
    }

    fn render_entries(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|ix| self.render_entry(ix, window, cx))
            .collect()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(theme.colors().border_variant)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(Icon::new(IconName::Terminal).size(IconSize::Small))
                    .child(Label::new(t("terminal_outline.title")).size(LabelSize::Small)),
            )
    }

    fn render_empty_state(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let message = if self.active_terminal.is_none() {
            t("terminal_outline.no_terminal")
        } else {
            t("terminal_outline.no_commands")
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .p_4()
            .child(
                Label::new(message)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }
}

impl EventEmitter<PanelEvent> for TerminalOutline {}

impl Render for TerminalOutline {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let item_count = self.entries.len();

        div()
            .id("terminal-outline")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle(cx))
            .child(self.render_header(cx))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(if item_count > 0 {
                        uniform_list(
                            "terminal-outline-list",
                            item_count,
                            cx.processor(|this, range: Range<usize>, window, cx| {
                                this.render_entries(range, window, cx)
                            }),
                        )
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll_handle)
                        .into_any_element()
                    } else {
                        self.render_empty_state(cx).into_any_element()
                    }),
            )
    }
}

impl Focusable for TerminalOutline {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for TerminalOutline {
    fn persistent_name() -> &'static str {
        "Terminal Outline"
    }

    fn panel_key() -> &'static str {
        TERMINAL_OUTLINE_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
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
        Some(IconName::ListTree)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("terminal_outline.title")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        15
    }
}
