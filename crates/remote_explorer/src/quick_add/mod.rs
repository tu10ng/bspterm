mod auto_recognize;
mod multi_connection_modal;
mod ssh_section;
mod telnet_section;

pub use auto_recognize::*;
pub use multi_connection_modal::*;
pub use ssh_section::*;
pub use telnet_section::*;

use std::collections::HashMap;

use gpui::{App, Entity, IntoElement, ParentElement, Styled, WeakEntity, Window};
use i18n::t;
use terminal::{SessionConfig, SessionGroup, SessionStoreEntity};
use ui::{prelude::*, Color, Disclosure, Label, LabelSize, h_flex, v_flex};
use workspace::{Pane, Workspace};

pub enum ConnectionResult {
    Ssh(terminal::SshSessionConfig, Entity<Workspace>, Entity<Pane>),
    Telnet(terminal::TelnetSessionConfig, Entity<Workspace>, Entity<Pane>),
}

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
                        Label::new(t("remote_explorer.quick_add"))
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
    ) -> Option<ConnectionResult> {
        let input = self.auto_recognize.get_input(cx);

        if is_session_env_info_format(&input) {
            let parsed = parse_connection_text(&input);
            if !parsed.is_empty() {
                self.import_session_env_info(parsed, cx);
            }
            self.auto_recognize.clear_input(window, cx);
            return None;
        }

        let parsed = parse_connection_text(&input);

        if parsed.is_empty() {
            return None;
        }

        let result = if parsed.len() == 1 {
            let connection = &parsed[0];
            self.connect_single(connection.clone(), workspace, pane, cx)
        } else {
            self.show_multi_connection_modal(parsed, workspace, pane, window, cx);
            None
        };

        self.auto_recognize.clear_input(window, cx);
        result
    }

    fn import_session_env_info(
        &mut self,
        connections: Vec<ParsedConnection>,
        cx: &mut App,
    ) {
        let mut groups: HashMap<String, Vec<ParsedConnection>> = HashMap::new();
        for conn in connections {
            groups.entry(conn.host.clone()).or_default().push(conn);
        }

        self.session_store.update(cx, |store, cx| {
            for (ip, conns) in groups {
                let group = SessionGroup::new(&ip);
                let group_id = group.id;
                store.add_group(group, None, cx);

                for conn in conns {
                    let session_config = create_session_config(&conn);
                    store.add_session(session_config, Some(group_id), cx);
                }
            }
        });
    }

    fn connect_single(
        &mut self,
        connection: ParsedConnection,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        cx: &mut App,
    ) -> Option<ConnectionResult> {
        match connection.protocol {
            ConnectionProtocol::Telnet => {
                let host = &connection.host;
                let port = connection.port;
                let config = terminal::TelnetSessionConfig::new(host, port);
                let config = if let (Some(user), Some(pass)) =
                    (&connection.username, &connection.password)
                {
                    config.with_credentials(user.clone(), pass.clone())
                } else {
                    config
                };

                let session_name = connection.name.clone().unwrap_or_else(|| {
                    if port == 23 {
                        connection.host.clone()
                    } else {
                        format!("{}:{}", connection.host, port)
                    }
                });
                let session_config =
                    terminal::SessionConfig::new_telnet(session_name, config.clone());

                let group_id = self.session_store.update(cx, |store, cx| {
                    store.get_or_create_group_by_name(&connection.host, cx)
                });

                self.session_store.update(cx, |store, cx| {
                    store.add_session(session_config, Some(group_id), cx);
                });

                if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
                    Some(ConnectionResult::Telnet(config, workspace, pane))
                } else {
                    None
                }
            }
            ConnectionProtocol::Ssh => {
                let username = connection.username.unwrap_or_else(|| "root".to_string());
                let password = connection.password.unwrap_or_else(|| "root".to_string());
                let host = &connection.host;
                let port = connection.port;

                let ssh_config = terminal::SshSessionConfig::new(host, port)
                    .with_username(&username)
                    .with_auth(terminal::AuthMethod::Password { password });

                let session_name = connection.name.clone().unwrap_or_else(|| {
                    if port == 22 {
                        connection.host.clone()
                    } else {
                        format!("{}:{}", connection.host, port)
                    }
                });
                let session_config =
                    terminal::SessionConfig::new_ssh(session_name, ssh_config.clone());

                let group_id = self.session_store.update(cx, |store, cx| {
                    store.get_or_create_group_by_name(&connection.host, cx)
                });

                self.session_store.update(cx, |store, cx| {
                    store.add_session(session_config, Some(group_id), cx);
                });

                if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
                    Some(ConnectionResult::Ssh(ssh_config, workspace, pane))
                } else {
                    None
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
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(terminal::TelnetSessionConfig, Entity<Workspace>, Entity<Pane>)> {
        let (host, port, username, password) = self.telnet_section.get_values(cx);

        if host.is_empty() {
            return None;
        }

        let port = port.parse::<u16>().unwrap_or(23);
        let config = terminal::TelnetSessionConfig::new(&host, port);
        let config = if !username.is_empty() {
            config.with_credentials(username, password)
        } else {
            config
        };

        let session_name = if port == 23 {
            host
        } else {
            format!("{}:{}", host, port)
        };

        let session_config = terminal::SessionConfig::new_telnet(session_name, config.clone());
        self.session_store.update(cx, |store, cx| {
            store.add_session(session_config, None, cx);
        });

        self.telnet_section.clear_fields(window, cx);

        if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
            Some((config, workspace, pane))
        } else {
            None
        }
    }

    pub fn handle_ssh_connect(
        &mut self,
        workspace: WeakEntity<Workspace>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(terminal::SshSessionConfig, Entity<Workspace>, Entity<Pane>)> {
        let host_input = self.ssh_section.get_host(cx);
        if host_input.is_empty() {
            return None;
        }

        let (host, port, username) = parse_ssh_host_string(&host_input);
        let username = username.unwrap_or_else(|| "root".to_string());
        let password = "root".to_string();

        let ssh_config = terminal::SshSessionConfig::new(&host, port)
            .with_username(&username)
            .with_auth(terminal::AuthMethod::Password { password });

        let session_name = if port == 22 {
            host
        } else {
            format!("{}:{}", host, port)
        };
        let session_config =
            terminal::SessionConfig::new_ssh(session_name, ssh_config.clone());

        self.session_store.update(cx, |store, cx| {
            store.add_session(session_config, None, cx);
        });

        self.ssh_section.clear_host(window, cx);

        if let (Some(workspace), Some(pane)) = (workspace.upgrade(), pane) {
            Some((ssh_config, workspace, pane))
        } else {
            None
        }
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

fn create_session_config(conn: &ParsedConnection) -> SessionConfig {
    match conn.protocol {
        ConnectionProtocol::Telnet => {
            let host = &conn.host;
            let port = conn.port;
            let config = terminal::TelnetSessionConfig::new(host, port);
            let config = if let (Some(user), Some(pass)) = (&conn.username, &conn.password) {
                config.with_credentials(user.clone(), pass.clone())
            } else {
                config
            };

            let session_name = conn.name.clone().unwrap_or_else(|| format_session_name(host, port, &None, 23));
            SessionConfig::new_telnet(session_name, config)
        }
        ConnectionProtocol::Ssh => {
            let host = &conn.host;
            let port = conn.port;
            let username = conn.username.clone().unwrap_or_else(|| "root".to_string());
            let password = conn.password.clone().unwrap_or_else(|| "root".to_string());

            let ssh_config = terminal::SshSessionConfig::new(host, port)
                .with_username(&username)
                .with_auth(terminal::AuthMethod::Password { password });

            let session_name = conn.name.clone().unwrap_or_else(|| format_session_name(host, port, &conn.username, 22));
            SessionConfig::new_ssh(session_name, ssh_config)
        }
    }
}

fn format_session_name(host: &str, port: u16, username: &Option<String>, default_port: u16) -> String {
    let host_part = if port == default_port {
        host.to_string()
    } else {
        format!("{}:{}", host, port)
    };

    if let Some(user) = username {
        format!("{}@{}", user, host_part)
    } else {
        host_part
    }
}

pub fn connect_ssh<T: 'static>(
    ssh_config: terminal::SshSessionConfig,
    session_id: Option<uuid::Uuid>,
    workspace: Entity<Workspace>,
    pane: Entity<Pane>,
    window: &mut Window,
    cx: &mut gpui::Context<T>,
) {
    use settings::Settings;
    use terminal::terminal_settings::TerminalSettings;
    use terminal::{ConnectionInfo, TerminalBuilder};
    use util::paths::PathStyle;

    let settings = TerminalSettings::get_global(cx);
    let cursor_shape = settings.cursor_shape;
    let alternate_scroll = settings.alternate_scroll;
    let max_scroll_history_lines = settings.max_scroll_history_lines;
    let path_style = PathStyle::local();
    let window_id = window.window_handle().window_id().as_u64();
    let weak_workspace = workspace.downgrade();
    let background_executor = cx.background_executor().clone();

    let connection_info = ConnectionInfo::Ssh {
        host: ssh_config.host.clone(),
        port: ssh_config.port,
        username: ssh_config.username.clone(),
        password: match &ssh_config.auth {
            terminal::AuthMethod::Password { password } => Some(password.clone()),
            _ => None,
        },
        private_key_path: match &ssh_config.auth {
            terminal::AuthMethod::PrivateKey { path, .. } => Some(path.clone()),
            _ => None,
        },
        passphrase: match &ssh_config.auth {
            terminal::AuthMethod::PrivateKey { passphrase, .. } => passphrase.clone(),
            _ => None,
        },
        session_id,
    };

    let terminal_builder = match TerminalBuilder::new_disconnected_ssh(
        connection_info,
        cursor_shape,
        alternate_scroll,
        max_scroll_history_lines,
        window_id,
        &background_executor,
        path_style,
    ) {
        Ok(builder) => builder,
        Err(error) => {
            log::error!("Failed to create disconnected SSH terminal: {}", error);
            return;
        }
    };

    let terminal_handle = cx.new(|cx| terminal_builder.subscribe(cx));

    terminal_handle.update(cx, |terminal, _cx| {
        terminal.set_initial_connecting();
    });

    terminal_handle.update(cx, |terminal, cx| {
        terminal.write_output(b"\x1b[36mConnecting...\x1b[0m\r\n", cx);
    });

    let terminal_view = Box::new(cx.new(|cx| {
        terminal_view::TerminalView::new(
            terminal_handle.clone(),
            weak_workspace.clone(),
            workspace.read(cx).database_id(),
            workspace.read(cx).project().downgrade(),
            window,
            cx,
        )
    }));

    pane.update(cx, |pane, cx| {
        pane.add_item(terminal_view, true, true, None, window, cx);
    });

    let reconnect_task = terminal_handle.read(cx).reconnect(cx);

    cx.spawn_in(window, async move |_, cx| {
        match reconnect_task.await {
            Ok(connection) => {
                terminal_handle
                    .update_in(cx, |terminal, _window, cx| {
                        terminal.clear_initial_connecting();
                        terminal.set_connection(connection, cx);
                        terminal.write_output(b"\x1b[32mConnected\x1b[0m\r\n", cx);
                    })
                    .ok();
            }
            Err(err) => {
                terminal_handle
                    .update_in(cx, |terminal, _window, cx| {
                        terminal.clear_initial_connecting();
                        let message = format!(
                            "\x1b[31mConnection failed: {}\x1b[0m\r\n\x1b[33mPress Enter to reconnect\x1b[0m\r\n",
                            err
                        );
                        terminal.write_output(message.as_bytes(), cx);
                    })
                    .ok();
            }
        }
    })
    .detach();
}

pub fn connect_telnet<T: 'static>(
    telnet_config: terminal::TelnetSessionConfig,
    session_id: Option<uuid::Uuid>,
    workspace: Entity<Workspace>,
    pane: Entity<Pane>,
    window: &mut Window,
    cx: &mut gpui::Context<T>,
) {
    use settings::Settings;
    use terminal::terminal_settings::TerminalSettings;
    use terminal::{ConnectionInfo, TerminalBuilder};
    use util::paths::PathStyle;

    let settings = TerminalSettings::get_global(cx);
    let cursor_shape = settings.cursor_shape;
    let alternate_scroll = settings.alternate_scroll;
    let max_scroll_history_lines = settings.max_scroll_history_lines;
    let path_style = PathStyle::local();
    let window_id = window.window_handle().window_id().as_u64();
    let weak_workspace = workspace.downgrade();
    let background_executor = cx.background_executor().clone();

    let connection_info = ConnectionInfo::Telnet {
        host: telnet_config.host,
        port: telnet_config.port,
        username: telnet_config.username,
        password: telnet_config.password,
        session_id,
    };

    let terminal_builder = match TerminalBuilder::new_disconnected_telnet(
        connection_info,
        cursor_shape,
        alternate_scroll,
        max_scroll_history_lines,
        window_id,
        &background_executor,
        path_style,
    ) {
        Ok(builder) => builder,
        Err(error) => {
            log::error!("Failed to create disconnected Telnet terminal: {}", error);
            return;
        }
    };

    let terminal_handle = cx.new(|cx| terminal_builder.subscribe(cx));

    terminal_handle.update(cx, |terminal, _cx| {
        terminal.set_initial_connecting();
    });

    terminal_handle.update(cx, |terminal, cx| {
        terminal.write_output(b"\x1b[36mConnecting...\x1b[0m\r\n", cx);
    });

    let terminal_view = Box::new(cx.new(|cx| {
        terminal_view::TerminalView::new(
            terminal_handle.clone(),
            weak_workspace.clone(),
            workspace.read(cx).database_id(),
            workspace.read(cx).project().downgrade(),
            window,
            cx,
        )
    }));

    pane.update(cx, |pane, cx| {
        pane.add_item(terminal_view, true, true, None, window, cx);
    });

    let reconnect_task = terminal_handle.read(cx).reconnect(cx);

    cx.spawn_in(window, async move |_, cx| {
        match reconnect_task.await {
            Ok(connection) => {
                terminal_handle
                    .update_in(cx, |terminal, _window, cx| {
                        terminal.clear_initial_connecting();
                        terminal.set_connection(connection, cx);
                        terminal.write_output(b"\x1b[32mConnected\x1b[0m\r\n", cx);
                    })
                    .ok();
            }
            Err(err) => {
                terminal_handle
                    .update_in(cx, |terminal, _window, cx| {
                        terminal.clear_initial_connecting();
                        let message = format!(
                            "\x1b[31mConnection failed: {}\x1b[0m\r\n\x1b[33mPress Enter to reconnect\x1b[0m\r\n",
                            err
                        );
                        terminal.write_output(message.as_bytes(), cx);
                    })
                    .ok();
            }
        }
    })
    .detach();
}
