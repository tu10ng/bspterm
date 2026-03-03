use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};
use i18n::t;
use std::path::PathBuf;
use terminal::{
    FunctionConfig, FunctionProtocol, FunctionStoreEntity, FunctionStoreEvent,
};
use uuid::Uuid;
use ui::{FluentBuilder, IconButton, IconName, IconSize, Label, Switch, TintColor, ToggleState, prelude::*};
use workspace::ModalView;

/// Configuration modal for the function bar.
pub struct FunctionBarConfigModal {
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ModalView for FunctionBarConfigModal {}

impl EventEmitter<DismissEvent> for FunctionBarConfigModal {}

impl FunctionBarConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &FunctionStoreEntity::global(cx),
            |_this, _, _event: &FunctionStoreEvent, cx| {
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

    fn toggle_function(id: Uuid, enabled: bool, cx: &mut App) {
        let Some(store) = FunctionStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.update_function(id, |func| func.enabled = enabled, cx);
        });
    }
}

impl Focusable for FunctionBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FunctionBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = FunctionStoreEntity::global(cx);
        let functions = store.read(cx).functions().to_vec();
        let function_enabled = store.read(cx).function_enabled();

        v_flex()
            .id("function-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(400.0))
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
                            .child(t("function.config_title")),
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
                    .child(div().text_sm().child(t("function.enable_invocation")))
                    .child(
                        Switch::new(
                            "function-enabled-switch",
                            if function_enabled {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .on_click(move |_state, _window, cx| {
                            if let Some(store) = FunctionStoreEntity::try_global(cx) {
                                store.update(cx, |store, cx| {
                                    store.toggle_function_enabled(cx);
                                });
                            }
                        }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .when(functions.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text_muted)
                                .child(t("function.empty_hint")),
                        )
                    })
                    .children(functions.iter().map(|func| {
                        let func_id = func.id;
                        let is_enabled = func.enabled;
                        let name = func.name.clone();
                        let script_path = func.script_path.display().to_string();
                        let protocol_label = func.protocol.label();

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
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().colors().text_muted)
                                            .child(script_path),
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
                                    SharedString::from(format!("func-switch-{}", func_id)),
                                    if is_enabled {
                                        ToggleState::Selected
                                    } else {
                                        ToggleState::Unselected
                                    },
                                )
                                .on_click(move |state, _window, cx| {
                                    let enabled = *state == ToggleState::Selected;
                                    Self::toggle_function(func_id, enabled, cx);
                                }),
                            )
                    })),
            )
    }
}

fn protocol_button(
    id: &str,
    protocol: FunctionProtocol,
    current: &FunctionProtocol,
    cx: &mut Context<AddFunctionModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

fn edit_protocol_button(
    id: &str,
    protocol: FunctionProtocol,
    current: &FunctionProtocol,
    cx: &mut Context<EditFunctionModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

/// Modal dialog for adding a new function.
pub struct AddFunctionModal {
    focus_handle: FocusHandle,
    name_editor: Entity<Editor>,
    script_path_editor: Entity<Editor>,
    protocol: FunctionProtocol,
}

impl ModalView for AddFunctionModal {}

impl EventEmitter<DismissEvent> for AddFunctionModal {}

impl Focusable for AddFunctionModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AddFunctionModal {
    pub fn new(
        default_protocol: FunctionProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let name_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_placeholder_text(&t("function.name_placeholder"), window, cx);
            ed
        });

        let script_path_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_placeholder_text(&t("function.script_path_placeholder"), window, cx);
            ed
        });

        Self {
            focus_handle,
            name_editor,
            script_path_editor,
            protocol: default_protocol,
        }
    }

    fn set_protocol(&mut self, protocol: FunctionProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);
        let script_path_str = self.script_path_editor.read(cx).text(cx);

        if name.is_empty() || script_path_str.is_empty() {
            return;
        }

        let script_path = PathBuf::from(script_path_str);

        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let func = FunctionConfig::with_protocol(name, script_path, self.protocol.clone());
            store.update(cx, |store, cx| {
                store.add_function(func, cx);
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for AddFunctionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("AddFunctionModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(400.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("function.add_title")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.name_label")).size(LabelSize::Small))
                            .child(self.name_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.script_path_label")).size(LabelSize::Small))
                            .child(self.script_path_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(protocol_button(
                                        "add-all",
                                        FunctionProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(protocol_button(
                                        "add-ssh",
                                        FunctionProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(protocol_button(
                                        "add-telnet",
                                        FunctionProtocol::Telnet,
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

/// Modal dialog for editing an existing function.
pub struct EditFunctionModal {
    focus_handle: FocusHandle,
    func_id: Uuid,
    name_editor: Entity<Editor>,
    script_path_editor: Entity<Editor>,
    protocol: FunctionProtocol,
}

impl ModalView for EditFunctionModal {}

impl EventEmitter<DismissEvent> for EditFunctionModal {}

impl Focusable for EditFunctionModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditFunctionModal {
    pub fn new(func_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = FunctionStoreEntity::global(cx);
        let func = store.read(cx).find_function(func_id);

        let (name_text, script_path_text, protocol) = func
            .map(|f| (f.name.clone(), f.script_path.display().to_string(), f.protocol.clone()))
            .unwrap_or_default();

        let name_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(name_text, window, cx);
            ed
        });

        let script_path_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(script_path_text, window, cx);
            ed
        });

        Self {
            focus_handle,
            func_id,
            name_editor,
            script_path_editor,
            protocol,
        }
    }

    fn set_protocol(&mut self, protocol: FunctionProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);
        let script_path_str = self.script_path_editor.read(cx).text(cx);

        if name.is_empty() || script_path_str.is_empty() {
            cx.emit(DismissEvent);
            return;
        }

        let script_path = PathBuf::from(script_path_str);

        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let func_id = self.func_id;
            let protocol = self.protocol.clone();
            store.update(cx, |store, cx| {
                store.update_function(
                    func_id,
                    move |func| {
                        func.name = name;
                        func.script_path = script_path;
                        func.protocol = protocol;
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

impl Render for EditFunctionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("EditFunctionModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(400.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("function.edit_title")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.name_label")).size(LabelSize::Small))
                            .child(self.name_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.script_path_label")).size(LabelSize::Small))
                            .child(self.script_path_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(edit_protocol_button(
                                        "edit-all",
                                        FunctionProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_protocol_button(
                                        "edit-ssh",
                                        FunctionProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_protocol_button(
                                        "edit-telnet",
                                        FunctionProtocol::Telnet,
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
