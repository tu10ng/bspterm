use crate::dock::{Dock, DockPosition};
use gpui::{
    App, Context, Corner, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use settings::SettingsStore;
use ui::{prelude::*, right_click_menu, ContextMenu, IconButton, Tooltip, v_flex};
use util::ResultExt as _;

pub struct ActivityBar {
    dock: Entity<Dock>,
    _subscriptions: Vec<Subscription>,
}

impl ActivityBar {
    pub fn new(dock: Entity<Dock>, cx: &mut Context<Self>) -> Self {
        let dock_subscription = cx.observe(&dock, |_, _, cx| cx.notify());
        let settings_subscription = cx.observe_global::<SettingsStore>(|_, cx| cx.notify());
        Self {
            dock,
            _subscriptions: vec![dock_subscription, settings_subscription],
        }
    }
}

impl Focusable for ActivityBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.dock.focus_handle(cx)
    }
}

impl Render for ActivityBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dock = self.dock.read(cx);
        let active_index = dock.active_panel_index();
        let is_open = dock.is_open();
        let dock_position = dock.position();
        let accent_color = cx.theme().colors().icon_accent;

        let buttons: Vec<_> = dock
            .panels()
            .enumerate()
            .filter_map(|(i, panel)| {
                let icon = panel.icon(window, cx)?;
                let icon_tooltip = panel
                    .icon_tooltip(window, cx)
                    .ok_or_else(|| {
                        anyhow::anyhow!("can't render a panel button without an icon tooltip")
                    })
                    .log_err()?;
                let name = panel.persistent_name();
                let panel = panel.clone();

                let is_active_button = Some(i) == active_index && is_open;
                let action = if is_active_button {
                    dock.toggle_action()
                } else {
                    panel.toggle_action(window, cx)
                };
                let tooltip: SharedString = icon_tooltip.into();

                let focus_handle = dock.focus_handle(cx);

                Some(
                    right_click_menu(name)
                        .menu(move |window, cx| {
                            const POSITIONS: [DockPosition; 3] = [
                                DockPosition::Left,
                                DockPosition::Right,
                                DockPosition::Bottom,
                            ];

                            ContextMenu::build(window, cx, |mut menu, _, cx| {
                                for position in POSITIONS {
                                    if position != dock_position
                                        && panel.position_is_valid(position, cx)
                                    {
                                        let panel = panel.clone();
                                        menu = menu.entry(
                                            format!("Dock {}", position.label()),
                                            None,
                                            move |window, cx| {
                                                panel.set_position(position, window, cx);
                                            },
                                        )
                                    }
                                }
                                menu
                            })
                        })
                        .anchor(Corner::TopLeft)
                        .attach(Corner::TopRight)
                        .trigger(move |is_active, _window, _cx| {
                            let action = action.boxed_clone();
                            let tooltip = tooltip.clone();
                            let focus_handle = focus_handle.clone();
                            div()
                                .relative()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .py_1()
                                .child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(2.))
                                        .when(is_active_button, |this| this.bg(accent_color)),
                                )
                                .child(
                                    IconButton::new((name, is_active_button as u64), icon)
                                        .icon_size(IconSize::Medium)
                                        .toggle_state(is_active_button)
                                        .on_click({
                                            let action = action.boxed_clone();
                                            move |_, window, cx| {
                                                window.focus(&focus_handle, cx);
                                                window.dispatch_action(action.boxed_clone(), cx)
                                            }
                                        })
                                        .when(!is_active, |this| {
                                            this.tooltip(move |_window, cx| {
                                                Tooltip::for_action(tooltip.clone(), &*action, cx)
                                            })
                                        }),
                                )
                        }),
                )
            })
            .collect();

        let has_buttons = !buttons.is_empty();

        v_flex()
            .h_full()
            .w(px(48.))
            .flex_shrink_0()
            .bg(cx.theme().colors().title_bar_background)
            .border_r_1()
            .border_color(cx.theme().colors().border)
            .pt_1()
            .gap_0p5()
            .when(has_buttons, |this| this.children(buttons))
    }
}
