mod rule_edit_modal;

use anyhow::Result;
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, ListSizingBehavior, ParentElement, Render, Styled,
    Subscription, UniformListScrollHandle, WeakEntity, Window, px, uniform_list,
};
use terminal::{
    AutomationRule, CredentialType, Protocol, RuleAction, RuleCondition, RuleStoreEntity,
    RuleStoreEvent, TriggerEvent,
};
use ui::{
    prelude::*, Button, ButtonCommon, ButtonStyle, Checkbox, Color, Icon, IconName, IconSize,
    Label, LabelSize, ListItem, ListItemSpacing, h_flex, v_flex,
};
use uuid::Uuid;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};
use bspterm_actions::rule_editor::ToggleFocus;

pub use rule_edit_modal::RuleEditModal;

const RULE_EDITOR_PANEL_KEY: &str = "RuleEditorPanel";

pub fn init(cx: &mut App) {
    RuleStoreEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<RuleEditor>(window, cx);
        });
    })
    .detach();
}

pub struct RuleEditor {
    rule_store: Entity<RuleStoreEntity>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    rules: Vec<AutomationRule>,
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    selected_rule_id: Option<Uuid>,
    _subscriptions: Vec<Subscription>,
}

impl RuleEditor {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rule_store = RuleStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let rule_store_subscription = cx.subscribe(&rule_store, |this, _, event, cx| match event {
            RuleStoreEvent::Changed
            | RuleStoreEvent::RuleAdded(_)
            | RuleStoreEvent::RuleRemoved(_) => {
                this.update_rules(cx);
            }
        });

        let mut this = Self {
            rule_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            rules: Vec::new(),
            workspace: weak_workspace,
            width: None,
            selected_rule_id: None,
            _subscriptions: vec![rule_store_subscription],
        };

        this.update_rules(cx);
        this
    }

    fn update_rules(&mut self, cx: &mut Context<Self>) {
        self.rules = self.rule_store.read(cx).rules().to_vec();
        cx.notify();
    }

    fn toggle_rule_enabled(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.rule_store.update(cx, |store, cx| {
            store.toggle_rule_enabled(id, cx);
        });
    }

    fn delete_rule(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.rule_store.update(cx, |store, cx| {
            store.remove_rule(id, cx);
        });
    }

    fn add_new_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let rule_store = self.rule_store.clone();
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    RuleEditModal::new_rule(rule_store, window, cx)
                });
            });
        }
    }

    fn edit_rule(&mut self, rule: &AutomationRule, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let rule_store = self.rule_store.clone();
            let rule = rule.clone();
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    RuleEditModal::edit_rule(rule_store, rule, window, cx)
                });
            });
        }
    }

    fn render_entries(
        &self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter_map(|ix| self.rules.get(ix).map(|rule| self.render_rule_item(rule, cx)))
            .collect()
    }

    fn render_rule_item(&self, rule: &AutomationRule, cx: &mut Context<Self>) -> AnyElement {
        let id = rule.id;
        let name = rule.name.clone();
        let enabled = rule.enabled;
        let _is_selected = self.selected_rule_id == Some(id);
        let trigger_text = match &rule.trigger {
            TriggerEvent::Wakeup => "Wakeup",
            TriggerEvent::Connected => "Connected",
            TriggerEvent::Disconnected => "Disconnected",
        };

        let condition_text = format_condition(&rule.condition);
        let _action_text = format_action(&rule.action);

        ListItem::new(ElementId::from(SharedString::from(format!("rule-{}", id))))
            .spacing(ListItemSpacing::Sparse)
            .start_slot(
                Checkbox::new(SharedString::from(format!("checkbox-{}", id)), enabled.into())
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.toggle_rule_enabled(id, cx);
                    })),
            )
            .child(
                v_flex()
                    .gap_0p5()
                    .child(Label::new(name).size(LabelSize::Default))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Label::new(trigger_text)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(condition_text)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .end_slot(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(SharedString::from(format!("edit-{}", id)), "Edit")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener({
                                let rule = rule.clone();
                                move |this, _, window, cx| {
                                    this.edit_rule(&rule, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("delete-{}", id)), "Delete")
                            .style(ButtonStyle::Subtle)
                            .color(Color::Error)
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.delete_rule(id, cx);
                            })),
                    ),
            )
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.selected_rule_id = Some(id);
                cx.notify();
            }))
            .into_any_element()
    }
}

fn format_condition(condition: &RuleCondition) -> String {
    match condition {
        RuleCondition::Pattern { pattern, .. } => {
            let truncated = if pattern.len() > 20 {
                format!("{}...", &pattern[..17])
            } else {
                pattern.clone()
            };
            format!("Pattern: {}", truncated)
        }
        RuleCondition::ConnectionType { protocol } => {
            let proto = match protocol {
                Protocol::Ssh => "SSH",
                Protocol::Telnet => "Telnet",
            };
            format!("Protocol: {}", proto)
        }
        RuleCondition::All { conditions } => {
            format!("All ({} conditions)", conditions.len())
        }
        RuleCondition::Any { conditions } => {
            format!("Any ({} conditions)", conditions.len())
        }
    }
}

fn format_action(action: &RuleAction) -> String {
    match action {
        RuleAction::SendText { text, .. } => {
            let truncated = if text.len() > 15 {
                format!("{}...", &text[..12])
            } else {
                text.clone()
            };
            format!("Send: {}", truncated)
        }
        RuleAction::SendCredential { credential_type } => {
            let cred = match credential_type {
                CredentialType::Username => "Username",
                CredentialType::Password => "Password",
            };
            format!("Send {}", cred)
        }
        RuleAction::RunPython { .. } => "Run Python".to_string(),
        RuleAction::Sequence { actions } => {
            format!("Sequence ({} actions)", actions.len())
        }
        RuleAction::Delay { milliseconds } => {
            format!("Delay {}ms", milliseconds)
        }
    }
}

impl Render for RuleEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rule_count = self.rules.len();

        v_flex()
            .key_context("RuleEditor")
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
                            .child(Icon::new(IconName::Cog).size(IconSize::Small))
                            .child(Label::new("Terminal Rules").size(LabelSize::Default)),
                    )
                    .child(
                        Button::new("add-rule", "Add Rule")
                            .style(ButtonStyle::Filled)
                            .icon(IconName::Plus)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_new_rule(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .flex_grow()
                    .child(if rule_count > 0 {
                        uniform_list(
                            "rule-list",
                            rule_count,
                            cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                                this.render_entries(range, window, cx)
                            }),
                        )
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll_handle)
                        .into_any_element()
                    } else {
                        v_flex()
                            .size_full()
                            .justify_center()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Cog)
                                    .size(IconSize::Medium)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new("No automation rules")
                                    .size(LabelSize::Default)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new("Add a rule to automate terminal actions")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .into_any_element()
                    }),
            )
    }
}

impl Focusable for RuleEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for RuleEditor {}

impl Panel for RuleEditor {
    fn persistent_name() -> &'static str {
        "Rule Editor"
    }

    fn panel_key() -> &'static str {
        RULE_EDITOR_PANEL_KEY
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
        self.width.unwrap_or(px(300.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Cog)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Terminal Rule Editor")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        11
    }
}
