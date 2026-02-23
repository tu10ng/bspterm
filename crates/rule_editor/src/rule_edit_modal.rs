use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window,
};
use i18n::t;
use terminal::{
    AutomationRule, CredentialType, Protocol, RuleAction, RuleCondition, RuleStoreEntity,
    TriggerEvent,
};
use ui::{
    prelude::*, Button, ButtonCommon, ButtonStyle, Label, LabelSize, h_flex, v_flex,
};
use uuid::Uuid;
use workspace::ModalView;

pub struct RuleEditModal {
    rule_store: Entity<RuleStoreEntity>,
    editing_rule_id: Option<Uuid>,
    name_editor: Entity<Editor>,
    pattern_editor: Entity<Editor>,
    send_text_editor: Entity<Editor>,
    trigger: TriggerEvent,
    max_triggers: Option<u32>,
    protocol: Protocol,
    case_insensitive: bool,
    action_type: ActionType,
    credential_type: CredentialType,
    append_newline: bool,
    focus_handle: FocusHandle,
}

#[derive(Clone, Copy, PartialEq)]
enum ActionType {
    SendCredential,
    SendText,
}

impl RuleEditModal {
    pub fn new_rule(
        rule_store: Entity<RuleStoreEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(t("rule_edit.title_new").to_string(), window, cx);
            editor.set_placeholder_text(&t("rule_edit.name_placeholder"), window, cx);
            editor
        });

        let pattern_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("rule_edit.pattern_placeholder"), window, cx);
            editor
        });

        let send_text_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("rule_edit.send_text_placeholder"), window, cx);
            editor
        });

        Self {
            rule_store,
            editing_rule_id: None,
            name_editor,
            pattern_editor,
            send_text_editor,
            trigger: TriggerEvent::Wakeup,
            max_triggers: Some(1),
            protocol: Protocol::Telnet,
            case_insensitive: true,
            action_type: ActionType::SendCredential,
            credential_type: CredentialType::Username,
            append_newline: true,
            focus_handle,
        }
    }

    pub fn edit_rule(
        rule_store: Entity<RuleStoreEntity>,
        rule: AutomationRule,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (protocol, pattern, case_insensitive) = extract_condition(&rule.condition);
        let (action_type, credential_type, send_text, append_newline) = extract_action(&rule.action);
        let focus_handle = cx.focus_handle();

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(rule.name.clone(), window, cx);
            editor.set_placeholder_text(&t("rule_edit.name_placeholder"), window, cx);
            editor
        });

        let pattern_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(pattern, window, cx);
            editor.set_placeholder_text(&t("rule_edit.pattern_placeholder"), window, cx);
            editor
        });

        let send_text_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(send_text, window, cx);
            editor.set_placeholder_text(&t("rule_edit.send_text_placeholder"), window, cx);
            editor
        });

        Self {
            rule_store,
            editing_rule_id: Some(rule.id),
            name_editor,
            pattern_editor,
            send_text_editor,
            trigger: rule.trigger,
            max_triggers: rule.max_triggers,
            protocol,
            case_insensitive,
            action_type,
            credential_type,
            append_newline,
            focus_handle,
        }
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);
        let pattern = self.pattern_editor.read(cx).text(cx);
        let send_text = self.send_text_editor.read(cx).text(cx);

        let condition = RuleCondition::All {
            conditions: vec![
                RuleCondition::ConnectionType {
                    protocol: self.protocol.clone(),
                },
                RuleCondition::Pattern {
                    pattern,
                    case_insensitive: self.case_insensitive,
                },
            ],
        };

        let action = match self.action_type {
            ActionType::SendCredential => RuleAction::SendCredential {
                credential_type: self.credential_type.clone(),
            },
            ActionType::SendText => RuleAction::SendText {
                text: send_text,
                append_newline: self.append_newline,
            },
        };

        if let Some(id) = self.editing_rule_id {
            let trigger = self.trigger.clone();
            let max_triggers = self.max_triggers;
            self.rule_store.update(cx, |store, cx| {
                store.update_rule(id, |rule| {
                    rule.name = name;
                    rule.trigger = trigger;
                    rule.max_triggers = max_triggers;
                    rule.condition = condition;
                    rule.action = action;
                }, cx);
            });
        } else {
            let rule = AutomationRule {
                id: Uuid::new_v4(),
                name,
                enabled: true,
                trigger: self.trigger.clone(),
                max_triggers: self.max_triggers,
                condition,
                action,
            };
            self.rule_store.update(cx, |store, cx| {
                store.add_rule(rule, cx);
            });
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn set_trigger(&mut self, trigger: TriggerEvent, cx: &mut Context<Self>) {
        self.trigger = trigger;
        cx.notify();
    }

    fn set_protocol(&mut self, protocol: Protocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn set_action_type(&mut self, action_type: ActionType, cx: &mut Context<Self>) {
        self.action_type = action_type;
        cx.notify();
    }

    fn set_credential_type(&mut self, credential_type: CredentialType, cx: &mut Context<Self>) {
        self.credential_type = credential_type;
        cx.notify();
    }
}

fn extract_condition(condition: &RuleCondition) -> (Protocol, String, bool) {
    let mut protocol = Protocol::Telnet;
    let mut pattern = String::new();
    let mut case_insensitive = true;

    fn extract_recursive(
        condition: &RuleCondition,
        protocol: &mut Protocol,
        pattern: &mut String,
        case_insensitive: &mut bool,
    ) {
        match condition {
            RuleCondition::ConnectionType { protocol: p } => {
                *protocol = p.clone();
            }
            RuleCondition::Pattern { pattern: p, case_insensitive: ci } => {
                *pattern = p.clone();
                *case_insensitive = *ci;
            }
            RuleCondition::All { conditions } | RuleCondition::Any { conditions } => {
                for c in conditions {
                    extract_recursive(c, protocol, pattern, case_insensitive);
                }
            }
        }
    }

    extract_recursive(condition, &mut protocol, &mut pattern, &mut case_insensitive);
    (protocol, pattern, case_insensitive)
}

fn extract_action(action: &RuleAction) -> (ActionType, CredentialType, String, bool) {
    match action {
        RuleAction::SendCredential { credential_type } => {
            (ActionType::SendCredential, credential_type.clone(), String::new(), true)
        }
        RuleAction::SendText { text, append_newline } => {
            (ActionType::SendText, CredentialType::Username, text.clone(), *append_newline)
        }
        RuleAction::Sequence { actions } if !actions.is_empty() => {
            extract_action(&actions[0])
        }
        _ => (ActionType::SendCredential, CredentialType::Username, String::new(), true),
    }
}

impl Render for RuleEditModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.editing_rule_id.is_some() {
            t("rule_edit.title_edit")
        } else {
            t("rule_edit.title_new")
        };

        let trigger = self.trigger.clone();
        let protocol = self.protocol.clone();
        let action_type = self.action_type;
        let credential_type = self.credential_type.clone();

        v_flex()
            .key_context("RuleEditModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(450.))
            .child(Label::new(title).size(LabelSize::Large))
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("common.name")).size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.name_editor.clone()),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(Label::new(t("rule_editor.trigger")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        Button::new("trigger-wakeup", t("rule_editor.trigger_wakeup"))
                                            .style(if trigger == TriggerEvent::Wakeup {
                                                ButtonStyle::Filled
                                            } else {
                                                ButtonStyle::Subtle
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.set_trigger(TriggerEvent::Wakeup, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("trigger-connected", t("rule_editor.trigger_connected"))
                                            .style(if trigger == TriggerEvent::Connected {
                                                ButtonStyle::Filled
                                            } else {
                                                ButtonStyle::Subtle
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.set_trigger(TriggerEvent::Connected, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(Label::new(t("session_edit.protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        Button::new("protocol-telnet", "Telnet")
                                            .style(if protocol == Protocol::Telnet {
                                                ButtonStyle::Filled
                                            } else {
                                                ButtonStyle::Subtle
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.set_protocol(Protocol::Telnet, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("protocol-ssh", "SSH")
                                            .style(if protocol == Protocol::Ssh {
                                                ButtonStyle::Filled
                                            } else {
                                                ButtonStyle::Subtle
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.set_protocol(Protocol::Ssh, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("rule_edit.pattern")).size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.pattern_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("rule_editor.action")).size(LabelSize::Small))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("action-credential", t("rule_edit.action_credential"))
                                    .style(if action_type == ActionType::SendCredential {
                                        ButtonStyle::Filled
                                    } else {
                                        ButtonStyle::Subtle
                                    })
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.set_action_type(ActionType::SendCredential, cx);
                                    })),
                            )
                            .child(
                                Button::new("action-text", t("rule_edit.action_text"))
                                    .style(if action_type == ActionType::SendText {
                                        ButtonStyle::Filled
                                    } else {
                                        ButtonStyle::Subtle
                                    })
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.set_action_type(ActionType::SendText, cx);
                                    })),
                            ),
                    )
                    .when(action_type == ActionType::SendCredential, |this| {
                        this.child(
                            h_flex()
                                .gap_1()
                                .mt_1()
                                .child(
                                    Button::new("cred-username", t("rule_edit.credential_username"))
                                        .style(if credential_type == CredentialType::Username {
                                            ButtonStyle::Filled
                                        } else {
                                            ButtonStyle::Subtle
                                        })
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.set_credential_type(CredentialType::Username, cx);
                                        })),
                                )
                                .child(
                                    Button::new("cred-password", t("rule_edit.credential_password"))
                                        .style(if credential_type == CredentialType::Password {
                                            ButtonStyle::Filled
                                        } else {
                                            ButtonStyle::Subtle
                                        })
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.set_credential_type(CredentialType::Password, cx);
                                        })),
                                ),
                        )
                    })
                    .when(action_type == ActionType::SendText, |this| {
                        this.child(
                            div()
                                .mt_1()
                                .border_1()
                                .border_color(cx.theme().colors().border)
                                .rounded_md()
                                .px_2()
                                .py_1()
                                .child(self.send_text_editor.clone()),
                        )
                    }),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .mt_2()
                    .child(
                        Button::new("cancel", t("common.cancel"))
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("save", t("common.save"))
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}

impl Focusable for RuleEditModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for RuleEditModal {}

impl ModalView for RuleEditModal {}
