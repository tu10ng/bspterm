use std::sync::Arc;

use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, Task, WeakEntity, Window,
};
use log_tracer::{
    AnalysisContext, AnalysisPipeline, AnalysisProgress, CodeServerConfig, LogParseRule,
    LogParseRuleStore, SshDockerCodeSource,
};
use ui::{
    prelude::*, Button, ButtonCommon, ButtonStyle, Color, Icon, IconName, IconSize, Label,
    LabelSize, h_flex, v_flex,
};
use workspace::{ModalView, Workspace};

use super::{CallGraphPanel, CodeServerModal};

pub struct TraceConfigModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    log_content: String,
    code_server_config: CodeServerConfig,
    selected_rule_index: usize,
    rules: Vec<LogParseRule>,
    _analysis_task: Option<Task<()>>,
}

impl TraceConfigModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        log_content: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let rule_store = LogParseRuleStore::load().unwrap_or_default();
        let rules = rule_store.rules().to_vec();
        let code_server_config = CodeServerConfig::load().unwrap_or_default();

        Self {
            focus_handle,
            workspace,
            log_content,
            code_server_config,
            selected_rule_index: 0,
            rules,
            _analysis_task: None,
        }
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rule) = self.rules.get(self.selected_rule_index).cloned() else {
            return;
        };

        let config = self.code_server_config.clone();
        let log_content = self.log_content.clone();
        let workspace = self.workspace.clone();

        log::info!(
            "[TraceCallGraph] Starting analysis with rule '{}', ssh: {}, container: {}",
            rule.name,
            config.display_host(),
            config.container_id
        );

        let task = cx.spawn_in(window, async move |_, cx| {
            // Open CallGraphPanel
            let panel = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_panel::<CallGraphPanel>(window, cx);
                    workspace.panel::<CallGraphPanel>(cx)
                })
                .ok()
                .flatten();

            let Some(panel) = panel else {
                log::error!("[TraceCallGraph] CallGraphPanel not available");
                return;
            };

            // Set progress: Parsing
            panel
                .update_in(cx, |panel, _window, cx| {
                    panel.set_progress(AnalysisProgress::Parsing { current: 0, total: 0 }, cx);
                })
                .ok();

            // Create code source
            let code_source = Arc::new(SshDockerCodeSource::new(config));

            // Build context
            let ctx_result = AnalysisContext::new(log_content).with_rule(rule);
            let mut analysis_ctx = match ctx_result {
                Ok(ctx) => ctx.with_code_source(code_source),
                Err(e) => {
                    panel
                        .update_in(cx, |panel, _window, cx| {
                            panel.set_error(format!("Failed to compile rule: {}", e), cx);
                        })
                        .ok();
                    return;
                }
            };

            // Run pipeline in background (it creates its own tokio runtime for SSH)
            let result = cx
                .background_executor()
                .spawn(async move {
                    let pipeline = AnalysisPipeline::default_pipeline();
                    pipeline.run(&mut analysis_ctx)?;
                    Ok::<_, anyhow::Error>(analysis_ctx.graph)
                })
                .await;

            // Update panel with result
            match result {
                Ok(Some(graph)) => {
                    panel
                        .update_in(cx, |panel, _window, cx| {
                            panel.set_graph(graph, cx);
                        })
                        .ok();
                }
                Ok(None) => {
                    panel
                        .update_in(cx, |panel, _window, cx| {
                            panel.set_error("No graph produced", cx);
                        })
                        .ok();
                }
                Err(e) => {
                    panel
                        .update_in(cx, |panel, _window, cx| {
                            panel.set_error(format!("Analysis failed: {}", e), cx);
                        })
                        .ok();
                }
            }
        });

        self._analysis_task = Some(task);
        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn open_config_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        workspace.update(cx, |_, cx| {
            cx.defer_in(window, move |workspace, window, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    CodeServerModal::new(window, cx)
                });
            });
        });
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        self.code_server_config = CodeServerConfig::load().unwrap_or_default();
        cx.notify();
    }
}

impl EventEmitter<DismissEvent> for TraceConfigModal {}

impl ModalView for TraceConfigModal {
    fn on_before_dismiss(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> workspace::DismissDecision {
        workspace::DismissDecision::Dismiss(true)
    }
}

impl Focusable for TraceConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TraceConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_count = self.log_content.lines().count();
        let config = &self.code_server_config;
        let is_configured = config.is_configured();

        v_flex()
            .key_context("TraceConfigModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w(px(500.))
            .p_4()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::Folder).size(IconSize::Medium))
                            .child(Label::new("Trace Call Graph").size(LabelSize::Large)),
                    )
                    .child(
                        Button::new("close", "")
                            .style(ButtonStyle::Subtle)
                            .icon(IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(Label::new("Log Preview").size(LabelSize::Default))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Label::new(format!("{} lines selected", line_count))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(Label::new("Parse Rule").size(LabelSize::Default))
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .children(self.rules.iter().enumerate().map(|(idx, rule)| {
                                let is_selected = idx == self.selected_rule_index;
                                Button::new(
                                    SharedString::from(format!("rule-{}", idx)),
                                    rule.name.clone(),
                                )
                                .style(if is_selected {
                                    ButtonStyle::Filled
                                } else {
                                    ButtonStyle::Subtle
                                })
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    this.selected_rule_index = idx;
                                    cx.notify();
                                }))
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(Label::new("Code Server").size(LabelSize::Default))
                            .child(
                                Button::new("configure", "Configure")
                                    .style(ButtonStyle::Subtle)
                                    .icon(IconName::Settings)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_config_modal(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_2()
                            .when(is_configured, |this| {
                                this.child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Label::new("Server:")
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                        .child(
                                            Label::new(config.display_host())
                                                .size(LabelSize::Small)
                                                .color(Color::Default),
                                        ),
                                )
                                .when_some(
                                    if config.container_id.is_empty() {
                                        None
                                    } else {
                                        Some(config.container_id.clone())
                                    },
                                    |this, container_id| {
                                        this.child(
                                            h_flex()
                                                .gap_2()
                                                .child(
                                                    Label::new("Container:")
                                                        .size(LabelSize::Small)
                                                        .color(Color::Muted),
                                                )
                                                .child(
                                                    Label::new(container_id)
                                                        .size(LabelSize::Small)
                                                        .color(Color::Default),
                                                ),
                                        )
                                    },
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Label::new("Code Path:")
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                        .child(
                                            Label::new(config.code_root.clone())
                                                .size(LabelSize::Small)
                                                .color(Color::Default),
                                        ),
                                )
                            })
                            .when(!is_configured, |this| {
                                this.child(
                                    Label::new("Not configured - click Configure to set up")
                                        .size(LabelSize::Small)
                                        .color(Color::Warning),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("reload", "")
                            .style(ButtonStyle::Subtle)
                            .icon(IconName::RotateCcw)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.reload_config(cx);
                            })),
                    )
                    .child(
                        Button::new("cancel", "Cancel")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("confirm", "Trace")
                            .style(ButtonStyle::Filled)
                            .disabled(!is_configured)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(window, cx);
                            })),
                    ),
            )
    }
}
