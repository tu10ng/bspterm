mod auto_recognize;
mod multi_connection_modal;
mod ssh_section;
mod telnet_section;

pub use auto_recognize::*;
pub use multi_connection_modal::*;
pub use ssh_section::*;
pub use telnet_section::*;

use gpui::{App, Entity, IntoElement, ParentElement, Styled, WeakEntity, Window};
use terminal::SessionStoreEntity;
use ui::{prelude::*, Color, Disclosure, Label, LabelSize, h_flex, v_flex};
use workspace::{Pane, Workspace};

pub struct QuickAddArea {
    expanded: bool,
    pub auto_recognize: AutoRecognizeSection,
    pub telnet_section: TelnetSection,
    pub ssh_section: SshSection,
    session_store: Entity<SessionStoreEntity>,
    #[allow(dead_code)]
    workspace: WeakEntity<Workspace>,
    #[allow(dead_code)]
    pane: Option<Entity<Pane>>,
}

impl QuickAddArea {
    pub fn new(
        session_store: Entity<SessionStoreEntity>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self {
            expanded: true,
            auto_recognize: AutoRecognizeSection::new(window, cx),
            telnet_section: TelnetSection::new(session_store.clone(), window, cx),
            ssh_section: SshSection::new(window, cx),
            session_store,
            workspace,
            pane: None,
        }
    }

    pub fn set_pane(&mut self, pane: Entity<Pane>) {
        self.pane = Some(pane);
    }

    pub fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn render(&mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let expanded = self.expanded;

        v_flex()
            .w_full()
            .border_b_1()
            .border_color(theme.colors().border_variant)
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.colors().ghost_element_hover))
                    .child(Disclosure::new("quick-add-disclosure", expanded))
                    .child(
                        Label::new("Quick Add")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .when(expanded, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .px_2()
                        .pb_2()
                        .gap_3()
                        .child(self.render_auto_recognize_section(window, cx))
                        .child(self.render_telnet_section(window, cx))
                        .child(self.render_ssh_section(window, cx)),
                )
            })
    }

    fn render_auto_recognize_section(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        self.auto_recognize.render(window, cx)
    }

    fn render_telnet_section(&mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.telnet_section.render(window, cx)
    }

    fn render_ssh_section(&mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.ssh_section.render(window, cx)
    }

    pub fn handle_auto_recognize_confirm(
        &mut self,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.auto_recognize.get_input(cx);
        let parsed = parse_connection_text(&input);

        if parsed.is_empty() {
            return;
        }

        if parsed.len() == 1 {
            let connection = &parsed[0];
            self.connect_single(connection.clone(), workspace, pane, window, cx);
        } else {
            self.show_multi_connection_modal(parsed, workspace, pane, window, cx);
        }

        self.auto_recognize.clear_input(window, cx);
    }

    fn connect_single(
        &mut self,
        connection: ParsedConnection,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match connection.protocol {
            ConnectionProtocol::Telnet => {
                let config = terminal::TelnetSessionConfig::new(&connection.host, connection.port);
                let config = if let (Some(user), Some(pass)) =
                    (&connection.username, &connection.password)
                {
                    config.with_credentials(user.clone(), pass.clone())
                } else {
                    config
                };

                let session_name = format!("{}:{}", connection.host, connection.port);
                let session_config =
                    terminal::SessionConfig::new_telnet(session_name.clone(), config);

                self.session_store.update(cx, |store, cx| {
                    store.add_session(session_config, None, cx);
                });

                log::info!("Telnet connection not yet implemented: {}", session_name);
            }
            ConnectionProtocol::Ssh => {
                let username = connection.username.unwrap_or_else(|| "root".to_string());
                let password = connection.password.unwrap_or_else(|| "root".to_string());

                let ssh_config = terminal::SshSessionConfig::new(&connection.host, connection.port)
                    .with_username(&username)
                    .with_auth(terminal::AuthMethod::Password { password });

                let session_name = format!("{}@{}:{}", username, connection.host, connection.port);
                let session_config =
                    terminal::SessionConfig::new_ssh(session_name, ssh_config.clone());

                self.session_store.update(cx, |store, cx| {
                    store.add_session(session_config, None, cx);
                });

                if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
                    connect_ssh(ssh_config, workspace, pane, window, cx);
                }
            }
        }
    }

    fn show_multi_connection_modal(
        &mut self,
        connections: Vec<ParsedConnection>,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(workspace_entity) = workspace.upgrade() else {
            return;
        };

        let session_store = self.session_store.clone();
        workspace_entity.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, |window, cx| {
                MultiConnectionModal::new(connections, session_store, pane, window, cx)
            });
        });
    }

    pub fn handle_telnet_connect(
        &mut self,
        _workspace: WeakEntity<Workspace>,
        _pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (host, port, username, password) = self.telnet_section.get_values(cx);

        if host.is_empty() {
            return;
        }

        let port = port.parse::<u16>().unwrap_or(23);
        let config = terminal::TelnetSessionConfig::new(&host, port);
        let config = if !username.is_empty() {
            config.with_credentials(username.clone(), password)
        } else {
            config
        };

        let session_name = if username.is_empty() {
            format!("{}:{}", host, port)
        } else {
            format!("{}@{}:{}", username, host, port)
        };

        let session_config = terminal::SessionConfig::new_telnet(session_name.clone(), config);
        self.session_store.update(cx, |store, cx| {
            store.add_session(session_config, None, cx);
        });

        log::info!("Telnet connection not yet implemented: {}", session_name);

        self.telnet_section.clear_fields(window, cx);
    }

    pub fn handle_ssh_connect(
        &mut self,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let host_input = self.ssh_section.get_host(cx);
        if host_input.is_empty() {
            return;
        }

        let (host, port, username) = parse_ssh_host_string(&host_input);
        let username = username.unwrap_or_else(|| "root".to_string());
        let password = "root".to_string();

        let ssh_config = terminal::SshSessionConfig::new(&host, port)
            .with_username(&username)
            .with_auth(terminal::AuthMethod::Password { password });

        let session_name = format!("{}@{}:{}", username, host, port);
        let session_config =
            terminal::SessionConfig::new_ssh(session_name, ssh_config.clone());

        self.session_store.update(cx, |store, cx| {
            store.add_session(session_config, None, cx);
        });

        if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
            connect_ssh(ssh_config, workspace, pane, window, cx);
        }

        self.ssh_section.clear_host(window, cx);
    }
}

fn parse_ssh_host_string(input: &str) -> (String, u16, Option<String>) {
    let input = input.trim();

    let (user_host, port) = if let Some((left, port_str)) = input.rsplit_once(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            (left, port)
        } else {
            (input, 22)
        }
    } else {
        (input, 22)
    };

    if let Some((username, host)) = user_host.split_once('@') {
        (host.to_string(), port, Some(username.to_string()))
    } else {
        (user_host.to_string(), port, None)
    }
}

fn connect_ssh(
    ssh_config: terminal::SshSessionConfig,
    _workspace: Entity<Workspace>,
    _pane: Entity<Pane>,
    _window: &mut Window,
    _cx: &mut App,
) {
    let username = ssh_config.username.as_deref().unwrap_or("root");
    let session_name = format!("{}@{}:{}", username, ssh_config.host, ssh_config.port);
    log::info!(
        "SSH connection requested: {} (use terminal panel SSH button for actual connection)",
        session_name
    );
}
