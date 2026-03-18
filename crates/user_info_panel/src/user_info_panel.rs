use std::net::IpAddr;
use std::time::Duration;

use anyhow::Result;
use bspterm_actions::user_info_panel::ToggleFocus;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, Styled, Task, WeakEntity, Window,
    px,
};
use ui::{prelude::*, Color, CopyButton, Icon, IconName, IconSize, Label, LabelSize, h_flex, v_flex};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const USER_INFO_PANEL_KEY: &str = "UserInfoPanel";

/// A network interface with its IP address.
#[derive(Clone, Debug)]
struct NetworkInterface {
    name: String,
    ip: IpAddr,
}

/// Detect all network interfaces with valid IP addresses.
fn detect_network_interfaces() -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();

    match get_if_addrs::get_if_addrs() {
        Ok(addrs) => {
            for iface in addrs {
                let ip = iface.ip();

                if ip.is_loopback() {
                    continue;
                }

                if let IpAddr::V4(v4) = ip {
                    if v4.is_link_local() {
                        continue;
                    }
                }

                interfaces.push(NetworkInterface {
                    name: iface.name.clone(),
                    ip,
                });
            }
        }
        Err(error) => {
            log::error!("Failed to detect network interfaces: {}", error);
        }
    }

    interfaces
}

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
    #[allow(dead_code)]
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    network_interfaces: Vec<NetworkInterface>,
    _network_refresh_task: Option<Task<()>>,
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

        let network_interfaces = detect_network_interfaces();

        let mut this = Self {
            focus_handle,
            workspace: weak_workspace,
            width: None,
            network_interfaces,
            _network_refresh_task: None,
        };

        this.start_network_refresh(window, cx);
        this
    }

    fn start_network_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._network_refresh_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                cx.update(|_window, cx| {
                    this.update(cx, |panel, cx| {
                        panel.network_interfaces = detect_network_interfaces();
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        }));
    }

    fn render_network_interfaces(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .p_3()
            .gap_2()
            .child(
                h_flex().gap_2().child(
                    Label::new("本机网络")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .when(self.network_interfaces.is_empty(), |this| {
                        this.child(
                            Label::new("未检测到网络接口")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .children(self.network_interfaces.iter().map(|iface| {
                        h_flex()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Server)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(iface.ip.to_string()).size(LabelSize::Small))
                            .child(
                                CopyButton::new(
                                    format!("copy-ip-{}", iface.name),
                                    iface.ip.to_string(),
                                )
                                .icon_size(IconSize::XSmall),
                            )
                            .child(
                                Label::new(format!("({})", iface.name))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                    })),
            )
    }
}

impl EventEmitter<PanelEvent> for UserInfoPanel {}

impl Render for UserInfoPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("user-info-panel")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .child(self.render_network_interfaces(cx))
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
        Some("网络信息")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        15
    }
}
