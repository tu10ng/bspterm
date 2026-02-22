use std::net::IpAddr;

use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};
use ui::{prelude::*, Button, ButtonStyle, Label, LabelSize, h_flex, v_flex};
use uuid::Uuid;
use workspace::ModalView;

use crate::{ChatMessage, LanMessagingEntity, LanMessagingEvent, UserIdentity};

pub struct ChatModal {
    target_user: UserIdentity,
    target_ip: IpAddr,
    session_context: Option<Uuid>,
    messages: Vec<ChatMessage>,
    message_editor: Entity<Editor>,
    messaging_entity: Entity<LanMessagingEntity>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatModal {
    pub fn new(
        target_user: UserIdentity,
        target_ip: IpAddr,
        session_context: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let messaging_entity = LanMessagingEntity::global(cx);

        let messages = messaging_entity
            .read(cx)
            .get_conversation(&target_user.employee_id, cx);

        let message_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Type a message...", window, cx);
            editor
        });

        let subscription =
            cx.subscribe(&messaging_entity, |this: &mut Self, _, event: &LanMessagingEvent, cx| {
                match event {
                    LanMessagingEvent::MessageReceived(msg) => {
                        if msg.from.employee_id == this.target_user.employee_id
                            || msg.to.employee_id == this.target_user.employee_id
                        {
                            this.messages.push(msg.clone());
                            cx.notify();
                        }
                    }
                    LanMessagingEvent::MessageSent(msg) => {
                        if msg.to.employee_id == this.target_user.employee_id {
                            this.messages.push(msg.clone());
                            cx.notify();
                        }
                    }
                }
            });

        Self {
            target_user,
            target_ip,
            session_context,
            messages,
            message_editor,
            messaging_entity,
            focus_handle,
            _subscriptions: vec![subscription],
        }
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.message_editor.read(cx).text(cx);
        if content.is_empty() {
            return;
        }

        let to = self.target_user.clone();
        let to_ip = self.target_ip;
        let session_context = self.session_context;

        self.messaging_entity.update(cx, |entity, cx| {
            entity
                .send_message(to_ip, to, content, session_context, cx)
                .detach_and_log_err(cx);
        });

        self.message_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for ChatModal {}

impl EventEmitter<DismissEvent> for ChatModal {}

impl Focusable for ChatModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let border_variant_color = theme.colors().border_variant;

        let my_employee_id = local_user::LocalUserStoreEntity::try_global(cx)
            .and_then(|store| store.read(cx).profile().map(|p| p.employee_id.clone()))
            .unwrap_or_default();

        v_flex()
            .key_context("ChatModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w_96()
            .h_96()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .w_6()
                                    .h_6()
                                    .rounded_full()
                                    .bg(theme.colors().element_selected)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Label::new(self.target_user.initials())
                                            .size(LabelSize::XSmall),
                                    ),
                            )
                            .child(Label::new(self.target_user.name.clone())),
                    )
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
                    .flex_1()
                    .w_full()
                    .p_2()
                    .gap_1()
                    .overflow_hidden()
                    .children(self.messages.iter().map(|msg| {
                        let is_from_me = msg.from.employee_id == my_employee_id;
                        let bg_color = if is_from_me {
                            theme.colors().element_selected
                        } else {
                            theme.colors().element_background
                        };

                        h_flex()
                            .w_full()
                            .when(is_from_me, |this| this.justify_end())
                            .when(!is_from_me, |this| this.justify_start())
                            .child(
                                div()
                                    .max_w_64()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(bg_color)
                                    .child(Label::new(msg.content.clone()).size(LabelSize::Small)),
                            )
                    })),
            )
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_t_1()
                    .border_color(border_variant_color)
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.message_editor.clone()),
                    )
                    .child(
                        Button::new("send", "")
                            .icon(IconName::Send)
                            .icon_size(IconSize::Small)
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.send_message(window, cx);
                            })),
                    ),
            )
    }
}
