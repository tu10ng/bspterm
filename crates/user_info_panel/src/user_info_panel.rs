use std::net::IpAddr;
use std::time::Duration;

use anyhow::Result;
use bspterm_actions::user_info_panel::ToggleFocus;
use gpui::{
    Action, App, AsyncWindowContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, Styled, Subscription, Task, WeakEntity, Window,
    px,
};
use lan_discovery::{DiscoveredUser, LanDiscoveryEntity, LanDiscoveryEvent};
use lan_messaging::{ChatModal, UserIdentity};
use local_user::LocalUserStoreEntity;
use ui::{prelude::*, Color, Icon, IconName, IconSize, Indicator, Label, LabelSize, h_flex, v_flex};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const USER_INFO_PANEL_KEY: &str = "UserInfoPanel";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<UserInfoPanel>(window, cx);
        });
    })
    .detach();
}

pub struct UserInfoPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    network_refresh_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl UserInfoPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let mut subscriptions = Vec::new();

        if let Some(local_user) = LocalUserStoreEntity::try_global(cx) {
            subscriptions.push(cx.subscribe(&local_user, |_this, _, _event, cx| {
                cx.notify();
            }));
        }

        if let Some(lan_discovery) = LanDiscoveryEntity::try_global(cx) {
            subscriptions.push(cx.subscribe(&lan_discovery, |_this, _, event, cx| match event {
                LanDiscoveryEvent::UserDiscovered(_)
                | LanDiscoveryEvent::UserUpdated(_)
                | LanDiscoveryEvent::UserOffline(_)
                | LanDiscoveryEvent::SessionsChanged => {
                    cx.notify();
                }
            }));
        }

        let mut this = Self {
            focus_handle,
            workspace: weak_workspace,
            width: None,
            network_refresh_task: None,
            _subscriptions: subscriptions,
        };

        this.start_network_refresh(window, cx);
        this
    }

    fn start_network_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.network_refresh_task = Some(cx.spawn_in(window, async move |_this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                cx.update(|_window, cx| {
                    if let Some(local_user) = LocalUserStoreEntity::try_global(cx) {
                        local_user.update(cx, |store, cx| {
                            store.refresh_network_interfaces(cx);
                        });
                    }
                })
                .ok();
            }
        }));
    }

    fn open_chat_with_user(
        &mut self,
        user: &DiscoveredUser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_user = UserIdentity::new(&user.employee_id, &user.name);
        let target_ip = user
            .ip_addresses
            .first()
            .copied()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    ChatModal::new(target_user, target_ip, None, window, cx)
                });
            });
        }
    }

    fn render_current_user(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let (profile, interfaces) = LocalUserStoreEntity::try_global(cx)
            .map(|store| {
                let store = store.read(cx);
                (store.profile().cloned(), store.network_interfaces().to_vec())
            })
            .unwrap_or((None, Vec::new()));

        v_flex()
            .w_full()
            .p_3()
            .gap_2()
            .border_b_1()
            .border_color(theme.colors().border_variant)
            .child(
                h_flex().gap_2().child(
                    Label::new("当前用户")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(self.render_avatar(
                        profile
                            .as_ref()
                            .map(|p| p.initials())
                            .as_deref()
                            .unwrap_or("?"),
                        gpui::rgb(0x3B82F6),
                    ))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(
                                        profile
                                            .as_ref()
                                            .map(|p| p.name.clone())
                                            .unwrap_or_else(|| "未登录".to_string()),
                                    ))
                                    .when_some(profile.as_ref(), |this, p| {
                                        this.child(
                                            Label::new(format!("({})", p.employee_id))
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(Indicator::dot().color(Color::Success))
                                    .child(
                                        Label::new("在线")
                                            .size(LabelSize::Small)
                                            .color(Color::Success),
                                    ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .children(interfaces.iter().map(|iface| {
                        h_flex()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Server)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(iface.ip.to_string()).size(LabelSize::Small))
                            .child(
                                Label::new(format!("({})", iface.name))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                    })),
            )
    }

    fn render_lan_users_header(
        &self,
        user_count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();

        h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .h(px(1.))
                    .bg(theme.colors().border_variant),
            )
            .child(
                Label::new(format!("局域网用户 ({})", user_count))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .flex_1()
                    .h(px(1.))
                    .bg(theme.colors().border_variant),
            )
    }

    fn render_discovered_user(
        &self,
        user: &DiscoveredUser,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_online = !user.is_expired();
        let elapsed = user.last_seen.elapsed();

        let bg_colors = [
            gpui::rgb(0x3B82F6),
            gpui::rgb(0x10B981),
            gpui::rgb(0xF59E0B),
            gpui::rgb(0xEF4444),
            gpui::rgb(0x8B5CF6),
        ];
        let avatar_color = bg_colors[index % bg_colors.len()];

        let user_for_chat = user.clone();

        v_flex()
            .w_full()
            .p_3()
            .gap_2()
            .border_b_1()
            .border_color(theme.colors().border_variant)
            .hover(|style| style.bg(theme.colors().ghost_element_hover))
            .child(
                h_flex()
                    .gap_3()
                    .child(self.render_avatar(&user.initials(), avatar_color))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(user.name.clone()))
                                    .child(
                                        Label::new(format!("({})", user.employee_id))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(Indicator::dot().color(if is_online {
                                        Color::Success
                                    } else {
                                        Color::Muted
                                    }))
                                    .child(
                                        Label::new(if is_online {
                                            "在线".to_string()
                                        } else {
                                            format!("离线 ({}秒前)", elapsed.as_secs())
                                        })
                                        .size(LabelSize::Small)
                                        .color(if is_online {
                                            Color::Success
                                        } else {
                                            Color::Muted
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        ui::Button::new(
                            SharedString::from(format!("chat-{}", user.instance_id)),
                            "聊天",
                        )
                        .style(ui::ButtonStyle::Subtle)
                        .size(ui::ButtonSize::Compact)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_chat_with_user(&user_for_chat, window, cx);
                        })),
                    ),
            )
            .child(
                v_flex()
                    .pl_9()
                    .gap_1()
                    .children(user.ip_addresses.iter().map(|ip| {
                        h_flex()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Server)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(ip.to_string()).size(LabelSize::Small))
                    })),
            )
            .when(!user.active_sessions.is_empty(), |this| {
                this.child(
                    v_flex()
                        .pl_9()
                        .gap_1()
                        .child(
                            Label::new("活动会话:")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .children(user.active_sessions.iter().map(|session| {
                            h_flex()
                                .gap_2()
                                .child(
                                    Icon::new(match session.protocol {
                                        lan_discovery::SessionProtocol::Ssh => IconName::LetterS,
                                        lan_discovery::SessionProtocol::Telnet => IconName::LetterT,
                                    })
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                                )
                                .child(
                                    Label::new(format!("{}:{}", session.host, session.port))
                                        .size(LabelSize::Small),
                                )
                        })),
                )
            })
    }

    fn render_avatar(&self, initials: &str, bg_color: gpui::Rgba) -> impl IntoElement {
        div()
            .w_8()
            .h_8()
            .rounded_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new(initials.to_string()).color(Color::Default))
    }

    fn render_empty_state(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Icon::new(IconName::Person)
                    .size(IconSize::XLarge)
                    .color(Color::Muted),
            )
            .child(
                Label::new("暂无局域网用户")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new("其他用户上线后将显示在这里")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }
}

impl EventEmitter<PanelEvent> for UserInfoPanel {}

impl Render for UserInfoPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let discovered_users: Vec<DiscoveredUser> = LanDiscoveryEntity::try_global(cx)
            .map(|discovery| discovery.read(cx).users().cloned().collect())
            .unwrap_or_default();

        let user_list_content = if discovered_users.is_empty() {
            self.render_empty_state(cx).into_any_element()
        } else {
            let user_elements: Vec<_> = discovered_users
                .iter()
                .enumerate()
                .map(|(idx, user)| self.render_discovered_user(user, idx, cx).into_any_element())
                .collect();

            v_flex()
                .id("discovered-users-list")
                .w_full()
                .flex_1()
                .overflow_y_scroll()
                .children(user_elements)
                .into_any_element()
        };

        v_flex()
            .id("user-info-panel")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .child(self.render_current_user(cx))
            .child(self.render_lan_users_header(discovered_users.len(), cx))
            .child(user_list_content)
    }
}

impl Focusable for UserInfoPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for UserInfoPanel {
    fn persistent_name() -> &'static str {
        "User Info"
    }

    fn panel_key() -> &'static str {
        USER_INFO_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
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
        self.width.unwrap_or(px(280.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Person)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("用户信息")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        15
    }
}
