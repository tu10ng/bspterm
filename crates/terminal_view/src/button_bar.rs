use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};
use std::path::PathBuf;
use terminal::{ButtonBarStoreEntity, ButtonBarStoreEvent};
use ui::{
    FluentBuilder, IconButton, IconName, IconSize, Tooltip, prelude::*,
};
use uuid::Uuid;
use workspace::ModalView;

use script_panel::script_runner::{ScriptRunner, ScriptStatus};

pub use script_panel::script_runner;

/// Button bar script runner state.
pub struct ButtonBarScriptRunner {
    runner: ScriptRunner,
    button_id: Uuid,
}

impl ButtonBarScriptRunner {
    pub fn new(script_path: PathBuf, socket_path: PathBuf, terminal_id: Option<String>, button_id: Uuid) -> Self {
        Self {
            runner: ScriptRunner::new(script_path, socket_path, terminal_id),
            button_id,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        self.runner.start()
    }

    pub fn stop(&mut self) {
        self.runner.stop();
    }

    pub fn button_id(&self) -> Uuid {
        self.button_id
    }

    pub fn status(&mut self) -> &ScriptStatus {
        self.runner.status()
    }

    pub fn read_output(&mut self) -> Option<String> {
        self.runner.read_output()
    }
}

/// Configuration modal for the button bar.
pub struct ButtonBarConfigModal {
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ModalView for ButtonBarConfigModal {}

impl EventEmitter<DismissEvent> for ButtonBarConfigModal {}

impl ButtonBarConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &ButtonBarStoreEntity::global(cx),
            |_this, _, _event: &ButtonBarStoreEvent, cx| {
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
}

impl Focusable for ButtonBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ButtonBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = ButtonBarStoreEntity::global(cx);
        let buttons = store.read(cx).buttons().to_vec();

        v_flex()
            .id("button-bar-config-modal")
            .elevation_3(cx)
            .p_4()
            .gap_4()
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
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Configure Button Bar"),
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
                v_flex()
                    .gap_2()
                    .when(buttons.is_empty(), |this| {
                        this.child(
                            div()
                                .text_color(cx.theme().colors().text_muted)
                                .child("No buttons configured. Add a button below."),
                        )
                    })
                    .children(buttons.iter().enumerate().map(|(index, button)| {
                        let button_id = button.id;
                        let label = button.label.clone();
                        let script_path = button.script_path.display().to_string();
                        let can_move_up = index > 0;
                        let can_move_down = index < buttons.len() - 1;
                        let store = store.clone();
                        let store_up = store.clone();
                        let store_down = store.clone();

                        h_flex()
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .bg(cx.theme().colors().element_background)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_0p5()
                                    .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).child(label))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().colors().text_muted)
                                            .child(script_path),
                                    ),
                            )
                            .when(can_move_up, |this| {
                                this.child(
                                    IconButton::new(
                                        SharedString::from(format!("move-up-{}", button_id)),
                                        IconName::ArrowUp,
                                    )
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Move Up"))
                                    .on_click(move |_, _window, cx| {
                                        store_up.update(cx, |store, cx| {
                                            store.move_button(button_id, index.saturating_sub(1), cx);
                                        });
                                    }),
                                )
                            })
                            .when(can_move_down, |this| {
                                this.child(
                                    IconButton::new(
                                        SharedString::from(format!("move-down-{}", button_id)),
                                        IconName::ArrowDown,
                                    )
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Move Down"))
                                    .on_click(move |_, _window, cx| {
                                        store_down.update(cx, |store, cx| {
                                            store.move_button(button_id, index + 2, cx);
                                        });
                                    }),
                                )
                            })
                            .child(
                                IconButton::new(
                                    SharedString::from(format!("remove-btn-{}", button_id)),
                                    IconName::Trash,
                                )
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Remove"))
                                .on_click(move |_, _window, cx| {
                                    store.update(cx, |store, cx| {
                                        store.remove_button(button_id, cx);
                                    });
                                }),
                            )
                    })),
            )
            .child(
                h_flex().gap_2().child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().colors().text_muted)
                        .child("Edit ~/.config/bspterm/button_bar.json to add buttons"),
                ),
            )
    }
}
