use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window,
};
use ui::{prelude::*, Button, ButtonStyle, Color, Label, LabelSize, h_flex, v_flex};
use workspace::ModalView;

use crate::{LocalUserProfile, LocalUserStoreEntity};

pub struct LocalLoginModal {
    employee_id_editor: Entity<Editor>,
    name_editor: Entity<Editor>,
    focus_handle: FocusHandle,
}

impl LocalLoginModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let employee_id_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Employee ID", window, cx);
            editor
        });

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Name", window, cx);
            editor
        });

        Self {
            employee_id_editor,
            name_editor,
            focus_handle,
        }
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let employee_id = self.employee_id_editor.read(cx).text(cx);
        let name = self.name_editor.read(cx).text(cx);

        if employee_id.is_empty() || name.is_empty() {
            return;
        }

        let profile = LocalUserProfile::new(employee_id, name);

        let user_store = LocalUserStoreEntity::global(cx);
        user_store.update(cx, |store, cx| {
            store.set_profile(profile, cx);
        });

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for LocalLoginModal {}

impl EventEmitter<DismissEvent> for LocalLoginModal {}

impl Focusable for LocalLoginModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LocalLoginModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let border_variant_color = theme.colors().border_variant;

        v_flex()
            .key_context("LocalLoginModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w_80()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .justify_between()
                    .child(Label::new("Login"))
                    .child(
                        Button::new("close", "")
                            .icon(IconName::Close)
                            .icon_size(IconSize::Small)
                            .style(ButtonStyle::Transparent)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                Label::new("Employee ID")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_sm()
                                    .px_1()
                                    .py_px()
                                    .child(self.employee_id_editor.clone()),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new("Name").size(LabelSize::Small).color(Color::Muted))
                            .child(
                                div()
                                    .w_full()
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_sm()
                                    .px_1()
                                    .py_px()
                                    .child(self.name_editor.clone()),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_t_1()
                    .border_color(border_variant_color)
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel", "Cancel")
                            .style(ButtonStyle::Transparent)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("save", "Login")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}
