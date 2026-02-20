use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, Subscription, Window,
};
use terminal::{ShortcutBarStoreEntity, ShortcutBarStoreEvent, ShortcutEntry};
use ui::{IconButton, IconName, IconSize, Switch, ToggleState, prelude::*};
use uuid::Uuid;
use workspace::ModalView;

/// Configuration modal for the shortcut bar.
pub struct ShortcutBarConfigModal {
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ModalView for ShortcutBarConfigModal {}

impl EventEmitter<DismissEvent> for ShortcutBarConfigModal {}

impl ShortcutBarConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &ShortcutBarStoreEntity::global(cx),
            |_this, _, _event: &ShortcutBarStoreEvent, cx| {
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

    fn toggle_shortcut(id: Uuid, enabled: bool, cx: &mut App) {
        let Some(store) = ShortcutBarStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.set_shortcut_enabled(id, enabled, cx);
        });
    }

    fn toggle_visibility(cx: &mut App) {
        let Some(store) = ShortcutBarStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.toggle_visibility(cx);
        });
    }
}

impl Focusable for ShortcutBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ShortcutBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = ShortcutBarStoreEntity::global(cx);
        let shortcuts: Vec<ShortcutEntry> = store.read(cx).shortcuts().to_vec();
        let show_bar = store.read(cx).show_shortcut_bar();

        v_flex()
            .id("shortcut-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(340.0))
            .max_h(px(400.0))
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
                            .child("快捷键栏配置"),
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
                    .child(div().text_sm().child("显示快捷键栏"))
                    .child(
                        Switch::new(
                            "show-shortcut-bar-switch",
                            if show_bar {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .on_click(move |_state, _window, cx| {
                            Self::toggle_visibility(cx);
                        }),
                    ),
            )
            .child(
                v_flex()
                    .id("shortcut-list-container")
                    .gap_1()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(shortcuts.iter().map(|shortcut| {
                        let shortcut_id = shortcut.id;
                        let is_enabled = shortcut.enabled;
                        let keybinding = shortcut.keybinding.clone();
                        let label = shortcut.label.clone();

                        h_flex()
                            .w_full()
                            .py_1()
                            .px_2()
                            .rounded_sm()
                            .justify_between()
                            .items_center()
                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(120.0))
                                            .text_sm()
                                            .text_color(cx.theme().colors().text_muted)
                                            .child(keybinding),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .child("→"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(label),
                                    ),
                            )
                            .child(
                                Switch::new(
                                    SharedString::from(format!("shortcut-switch-{}", shortcut_id)),
                                    if is_enabled {
                                        ToggleState::Selected
                                    } else {
                                        ToggleState::Unselected
                                    },
                                )
                                .on_click(move |state, _window, cx| {
                                    let enabled = *state == ToggleState::Selected;
                                    Self::toggle_shortcut(shortcut_id, enabled, cx);
                                }),
                            )
                    })),
            )
    }
}
