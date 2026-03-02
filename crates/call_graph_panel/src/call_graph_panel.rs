mod code_server_modal;
mod svg_view;
mod trace_config_modal;

use anyhow::Result;
use gpui::{
    Action, AnyElement, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription,
    WeakEntity, Window, px,
};
use i18n::t;
use log_tracer::{AnalysisProgress, CallGraph, DotOptions, render_dot};
use ui::{
    prelude::*, Button, ButtonCommon, ButtonStyle, Color, Icon, IconName, IconSize, Label,
    LabelSize, h_flex, v_flex,
};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};
use bspterm_actions::call_graph_panel::ToggleFocus;

pub use code_server_modal::CodeServerModal;
pub use svg_view::SvgView;
pub use trace_config_modal::TraceConfigModal;

const CALL_GRAPH_PANEL_KEY: &str = "CallGraphPanel";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<CallGraphPanel>(window, cx);
        });
    })
    .detach();
}

pub struct CallGraphPanel {
    focus_handle: FocusHandle,
    _workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    graph: Option<CallGraph>,
    dot_content: Option<String>,
    svg_content: Option<String>,
    progress: AnalysisProgress,
    error_message: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl CallGraphPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        Self {
            focus_handle,
            _workspace: weak_workspace,
            width: None,
            graph: None,
            dot_content: None,
            svg_content: None,
            progress: AnalysisProgress::Idle,
            error_message: None,
            _subscriptions: Vec::new(),
        }
    }

    pub fn set_graph(&mut self, graph: CallGraph, cx: &mut Context<Self>) {
        log::info!(
            "[CallGraphPanel] Setting graph with {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );

        let options = DotOptions::default();
        let dot = render_dot(&graph, &options);
        self.dot_content = Some(dot.clone());

        self.graph = Some(graph);
        self.progress = AnalysisProgress::Complete;
        self.error_message = None;

        cx.notify();
    }

    pub fn set_progress(&mut self, progress: AnalysisProgress, cx: &mut Context<Self>) {
        self.progress = progress;
        cx.notify();
    }

    pub fn set_error(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        self.error_message = Some(error.into());
        self.progress = AnalysisProgress::Error;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.graph = None;
        self.dot_content = None;
        self.svg_content = None;
        self.progress = AnalysisProgress::Idle;
        self.error_message = None;
        cx.notify();
    }

    fn render_progress(&self, _cx: &mut Context<Self>) -> AnyElement {
        let (message, show_spinner) = match self.progress {
            AnalysisProgress::Idle => (t("call_graph.idle"), false),
            AnalysisProgress::Parsing { current, total } => {
                (format!("Parsing log: {}/{}", current, total).into(), true)
            }
            AnalysisProgress::Searching { current, total } => {
                (format!("Searching code: {}/{}", current, total).into(), true)
            }
            AnalysisProgress::Building => (t("call_graph.building"), true),
            AnalysisProgress::Rendering => (t("call_graph.rendering"), true),
            AnalysisProgress::Complete => (t("call_graph.complete"), false),
            AnalysisProgress::Error => (t("call_graph.error"), false),
        };

        v_flex()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .child(
                Icon::new(if show_spinner {
                    IconName::ArrowCircle
                } else {
                    IconName::Folder
                })
                .size(IconSize::Medium)
                .color(Color::Muted),
            )
            .child(
                Label::new(message)
                    .size(LabelSize::Default)
                    .color(Color::Muted),
            )
            .into_any_element()
    }

    fn render_error(&self, cx: &mut Context<Self>) -> AnyElement {
        let error = self.error_message.clone().unwrap_or_else(|| "Unknown error".to_string());

        v_flex()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .child(
                Icon::new(IconName::XCircle)
                    .size(IconSize::Medium)
                    .color(Color::Error),
            )
            .child(
                Label::new(t("call_graph.error_title"))
                    .size(LabelSize::Default)
                    .color(Color::Error),
            )
            .child(
                Label::new(error)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Button::new("retry", t("common.retry"))
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.clear(cx);
                    })),
            )
            .into_any_element()
    }

    fn render_graph(&self, _cx: &mut Context<Self>) -> AnyElement {
        if let Some(ref dot) = self.dot_content {
            v_flex()
                .size_full()
                .p_2()
                .child(
                    v_flex()
                        .id("dot-content")
                        .size_full()
                        .bg(gpui::rgb(0xf5f5f5))
                        .rounded_md()
                        .p_2()
                        .overflow_y_scroll()
                        .child(
                            Label::new(dot.clone())
                                .size(LabelSize::XSmall)
                                .color(Color::Default),
                        ),
                )
                .into_any_element()
        } else {
            self.render_progress(_cx)
        }
    }
}

impl Render for CallGraphPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_graph = self.graph.is_some();

        v_flex()
            .key_context("CallGraphPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_1()
                            .child(Icon::new(IconName::Folder).size(IconSize::Small))
                            .child(Label::new(t("call_graph.title")).size(LabelSize::Default)),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .when(has_graph, |this| {
                                this.child(
                                    Button::new("export-dot", t("call_graph.export_dot"))
                                        .style(ButtonStyle::Subtle)
                                        .icon(IconName::Download)
                                        .icon_size(IconSize::Small)
                                        .on_click(cx.listener(|this, _, _window, _cx| {
                                            if let Some(ref dot) = this.dot_content {
                                                log::info!(
                                                    "[CallGraphPanel] DOT export:\n{}",
                                                    dot
                                                );
                                            }
                                        })),
                                )
                            })
                            .when(has_graph, |this| {
                                this.child(
                                    Button::new("clear", t("common.clear"))
                                        .style(ButtonStyle::Subtle)
                                        .icon(IconName::Trash)
                                        .icon_size(IconSize::Small)
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.clear(cx);
                                        })),
                                )
                            }),
                    ),
            )
            .child(
                v_flex()
                    .flex_grow()
                    .child(match self.progress {
                        AnalysisProgress::Error => self.render_error(cx),
                        AnalysisProgress::Complete if has_graph => self.render_graph(cx),
                        _ => self.render_progress(cx),
                    }),
            )
    }
}

impl Focusable for CallGraphPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for CallGraphPanel {}

impl Panel for CallGraphPanel {
    fn persistent_name() -> &'static str {
        "Call Graph"
    }

    fn panel_key() -> &'static str {
        CALL_GRAPH_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right | DockPosition::Bottom)
    }

    fn set_position(&mut self, _position: DockPosition, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(400.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, _cx: &mut Context<Self>) {
        self.width = size;
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Folder)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Call Graph Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        13
    }
}
