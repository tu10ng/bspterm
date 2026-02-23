use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};
use i18n::t;
use terminal::{
    Abbreviation, AbbreviationProtocol, AbbreviationStoreEntity, AbbreviationStoreEvent,
};
use uuid::Uuid;
use ui::{FluentBuilder, IconButton, IconName, IconSize, Label, Switch, ToggleState, prelude::*};
use workspace::ModalView;

/// Configuration modal for the abbreviation bar.
pub struct AbbrBarConfigModal {
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ModalView for AbbrBarConfigModal {}

impl EventEmitter<DismissEvent> for AbbrBarConfigModal {}

impl AbbrBarConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &AbbreviationStoreEntity::global(cx),
            |_this, _, _event: &AbbreviationStoreEvent, cx| {
                cx.notify();
            },
        );

        Self {
            focus_handle,
            _subscription: subscription,
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn toggle_abbreviation(id: Uuid, enabled: bool, cx: &mut App) {
        let Some(store) = AbbreviationStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.update_abbreviation(id, |abbr| abbr.enabled = enabled, cx);
        });
    }
}

impl Focusable for AbbrBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AbbrBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = AbbreviationStoreEntity::global(cx);
        let abbreviations = store.read(cx).abbreviations().to_vec();
        let expansion_enabled = store.read(cx).expansion_enabled();

        v_flex()
            .id("abbr-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(360.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| {
                this.dismiss(window, cx);
            }))
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(t("abbr.config_title")),
                    )
                    .child(
                        IconButton::new("close-modal", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .py_1()
                    .px_2()
                    .rounded_sm()
                    .justify_between()
                    .bg(cx.theme().colors().element_background)
                    .child(div().text_sm().child(t("abbr.enable_expansion")))
                    .child(
                        Switch::new(
                            "expansion-enabled-switch",
                            if expansion_enabled {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .on_click(move |_state, _window, cx| {
                            if let Some(store) = AbbreviationStoreEntity::try_global(cx) {
                                store.update(cx, |store, cx| {
                                    store.toggle_expansion(cx);
                                });
                            }
                        }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .when(abbreviations.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text_muted)
                                .child(t("abbr.empty_hint")),
                        )
                    })
                    .children(abbreviations.iter().map(|abbr| {
                        let abbr_id = abbr.id;
                        let is_enabled = abbr.enabled;
                        let trigger = abbr.trigger.clone();
                        let expansion = abbr.expansion.clone();
                        let protocol_label = abbr.protocol.label();

                        h_flex()
                            .py_1()
                            .px_2()
                            .rounded_sm()
                            .justify_between()
                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(format!("{} → {}", trigger, expansion)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_1()
                                            .rounded_sm()
                                            .bg(cx.theme().colors().element_background)
                                            .text_color(cx.theme().colors().text_muted)
                                            .child(protocol_label),
                                    ),
                            )
                            .child(
                                Switch::new(
                                    SharedString::from(format!("abbr-switch-{}", abbr_id)),
                                    if is_enabled {
                                        ToggleState::Selected
                                    } else {
                                        ToggleState::Unselected
                                    },
                                )
                                .on_click(move |state, _window, cx| {
                                    let enabled = *state == ToggleState::Selected;
                                    Self::toggle_abbreviation(abbr_id, enabled, cx);
                                }),
                            )
                    })),
            )
    }
}

fn protocol_button(
    id: &str,
    protocol: AbbreviationProtocol,
    current: &AbbreviationProtocol,
    cx: &mut Context<AddAbbrModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Filled
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

fn edit_protocol_button(
    id: &str,
    protocol: AbbreviationProtocol,
    current: &AbbreviationProtocol,
    cx: &mut Context<EditAbbrModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Filled
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

/// Modal dialog for adding a new abbreviation.
pub struct AddAbbrModal {
    focus_handle: FocusHandle,
    trigger_editor: Entity<Editor>,
    expansion_editor: Entity<Editor>,
    protocol: AbbreviationProtocol,
}

impl ModalView for AddAbbrModal {}

impl EventEmitter<DismissEvent> for AddAbbrModal {}

impl Focusable for AddAbbrModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AddAbbrModal {
    pub fn new(
        default_protocol: AbbreviationProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let trigger_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_placeholder_text(&t("abbr.trigger_placeholder"), window, cx);
            ed
        });

        let expansion_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_placeholder_text(&t("abbr.expansion_placeholder"), window, cx);
            ed
        });

        Self {
            focus_handle,
            trigger_editor,
            expansion_editor,
            protocol: default_protocol,
        }
    }

    fn set_protocol(&mut self, protocol: AbbreviationProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.trigger_editor.read(cx).text(cx);
        let expansion = self.expansion_editor.read(cx).text(cx);

        if trigger.is_empty() || expansion.is_empty() {
            return;
        }

        if let Some(store) = AbbreviationStoreEntity::try_global(cx) {
            let abbr = Abbreviation::with_protocol(trigger, expansion, self.protocol.clone());
            store.update(cx, |store, cx| {
                store.add_abbreviation(abbr, cx);
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for AddAbbrModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("AddAbbrModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(320.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("abbr.add_title")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.trigger_word")).size(LabelSize::Small))
                            .child(self.trigger_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.expansion_text")).size(LabelSize::Small))
                            .child(self.expansion_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(protocol_button(
                                        "add-all",
                                        AbbreviationProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(protocol_button(
                                        "add-ssh",
                                        AbbreviationProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(protocol_button(
                                        "add-telnet",
                                        AbbreviationProtocol::Telnet,
                                        &current_protocol,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", t("common.cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(&menu::Cancel, window, cx)
                            })),
                    )
                    .child(
                        ui::Button::new("confirm", t("common.confirm"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(&menu::Confirm, window, cx)
                            })),
                    ),
            )
    }
}

/// Modal dialog for editing an existing abbreviation.
pub struct EditAbbrModal {
    focus_handle: FocusHandle,
    abbr_id: Uuid,
    trigger_editor: Entity<Editor>,
    expansion_editor: Entity<Editor>,
    protocol: AbbreviationProtocol,
}

impl ModalView for EditAbbrModal {}

impl EventEmitter<DismissEvent> for EditAbbrModal {}

impl Focusable for EditAbbrModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditAbbrModal {
    pub fn new(abbr_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = AbbreviationStoreEntity::global(cx);
        let abbr = store.read(cx).find_abbreviation(abbr_id);

        let (trigger_text, expansion_text, protocol) = abbr
            .map(|a| (a.trigger.clone(), a.expansion.clone(), a.protocol.clone()))
            .unwrap_or_default();

        let trigger_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(trigger_text, window, cx);
            ed
        });

        let expansion_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(expansion_text, window, cx);
            ed
        });

        Self {
            focus_handle,
            abbr_id,
            trigger_editor,
            expansion_editor,
            protocol,
        }
    }

    fn set_protocol(&mut self, protocol: AbbreviationProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.trigger_editor.read(cx).text(cx);
        let expansion = self.expansion_editor.read(cx).text(cx);

        if trigger.is_empty() || expansion.is_empty() {
            cx.emit(DismissEvent);
            return;
        }

        if let Some(store) = AbbreviationStoreEntity::try_global(cx) {
            let abbr_id = self.abbr_id;
            let protocol = self.protocol.clone();
            store.update(cx, |store, cx| {
                store.update_abbreviation(
                    abbr_id,
                    move |abbr| {
                        abbr.trigger = trigger;
                        abbr.expansion = expansion;
                        abbr.protocol = protocol;
                    },
                    cx,
                );
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for EditAbbrModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("EditAbbrModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(320.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("abbr.edit_title")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.trigger_word")).size(LabelSize::Small))
                            .child(self.trigger_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.expansion_text")).size(LabelSize::Small))
                            .child(self.expansion_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("abbr.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(edit_protocol_button(
                                        "edit-all",
                                        AbbreviationProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_protocol_button(
                                        "edit-ssh",
                                        AbbreviationProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_protocol_button(
                                        "edit-telnet",
                                        AbbreviationProtocol::Telnet,
                                        &current_protocol,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", t("common.cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(&menu::Cancel, window, cx)
                            })),
                    )
                    .child(
                        ui::Button::new("confirm", t("common.confirm"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(&menu::Confirm, window, cx)
                            })),
                    ),
            )
    }
}
