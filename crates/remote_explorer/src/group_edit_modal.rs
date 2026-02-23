use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window,
};
use i18n::t;
use terminal::{SessionGroup, SessionNode, SessionStoreEntity};
use ui::{prelude::*, Button, ButtonStyle, Color, Label, LabelSize, h_flex, v_flex};
use uuid::Uuid;
use workspace::ModalView;

pub struct GroupEditModal {
    group_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    session_store: Entity<SessionStoreEntity>,
    name_editor: Entity<Editor>,
    focus_handle: FocusHandle,
}

impl GroupEditModal {
    pub fn new_create(
        parent_id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_store = SessionStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("group_edit.group_name_placeholder"), window, cx);
            editor
        });

        window.focus(&name_editor.focus_handle(cx), cx);

        Self {
            group_id: None,
            parent_id,
            session_store,
            name_editor,
            focus_handle,
        }
    }

    pub fn new_edit(group_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session_store = SessionStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let name = {
            let store = session_store.read(cx);
            if let Some(SessionNode::Group(group)) = store.store().find_node(group_id) {
                group.name.clone()
            } else {
                String::new()
            }
        };

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(name, window, cx);
            editor.set_placeholder_text(&t("group_edit.group_name_placeholder"), window, cx);
            editor
        });

        window.focus(&name_editor.focus_handle(cx), cx);

        Self {
            group_id: Some(group_id),
            parent_id: None,
            session_store,
            name_editor,
            focus_handle,
        }
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx).trim().to_string();
        if name.is_empty() {
            return;
        }

        if let Some(group_id) = self.group_id {
            self.session_store.update(cx, |store, cx| {
                store.update_group(group_id, |group| {
                    group.name = name;
                }, cx);
            });
        } else {
            let group = SessionGroup::new(name);
            self.session_store.update(cx, |store, cx| {
                store.add_group(group, self.parent_id, cx);
            });
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for GroupEditModal {}

impl EventEmitter<DismissEvent> for GroupEditModal {}

impl Focusable for GroupEditModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GroupEditModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let border_variant_color = theme.colors().border_variant;

        let title = if self.group_id.is_some() {
            t("group_edit.title_rename")
        } else {
            t("group_edit.title_new")
        };

        v_flex()
            .key_context("GroupEditModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w_80()
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                this.save(window, cx);
            }))
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .justify_between()
                    .child(Label::new(title))
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
                            .child(Label::new(t("common.name")).size(LabelSize::Small).color(Color::Muted))
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
                    .gap_2()
                    .justify_end()
                    .border_t_1()
                    .border_color(border_variant_color)
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
