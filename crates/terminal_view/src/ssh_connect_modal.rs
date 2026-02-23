use crate::TerminalView;
use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render,
    Styled, Task, WeakEntity, Window,
};
use i18n::t;
use settings::Settings;
use terminal::{TerminalBuilder, connection::ssh::{SshAuthConfig, SshConfig}, terminal_settings::TerminalSettings};
use ui::prelude::*;
use ui::SpinnerLabel;
use util::paths::PathStyle;
use workspace::{ModalView, Pane, Workspace};

#[derive(Clone, Debug, PartialEq)]
enum ConnectionStatus {
    Idle,
    Connecting,
    Error(SharedString),
}

pub struct SshConnectModal {
    workspace: WeakEntity<Workspace>,
    pane: Entity<Pane>,
    editor: Entity<Editor>,
    connection_status: ConnectionStatus,
    _connecting_task: Option<Task<()>>,
}

impl SshConnectModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("ssh_connect.placeholder"), window, cx);
            editor
        });

        cx.subscribe_in(&editor, window, Self::on_editor_event)
            .detach();

        Self {
            workspace,
            pane,
            editor,
            connection_status: ConnectionStatus::Idle,
            _connecting_task: None,
        }
    }

    fn on_editor_event(
        &mut self,
        _: &Entity<Editor>,
        event: &editor::EditorEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let editor::EditorEvent::BufferEdited { .. } = event {
            if matches!(self.connection_status, ConnectionStatus::Error(_)) {
                self.connection_status = ConnectionStatus::Idle;
                cx.notify();
            }
        }
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.connection_status, ConnectionStatus::Connecting) {
            return;
        }

        let input = self.editor.read(cx).text(cx);
        match parse_ssh_string(&input) {
            Ok(config) => {
                self.connection_status = ConnectionStatus::Connecting;
                self.editor.update(cx, |editor, cx| {
                    editor.set_read_only(true);
                    cx.notify();
                });
                cx.notify();
                self.connect(config, window, cx);
            }
            Err(err) => {
                self.connection_status = ConnectionStatus::Error(err.into());
                cx.notify();
            }
        }
    }

    fn cancel(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn connect(&mut self, config: SshConfig, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let settings = TerminalSettings::get_global(cx);
        let cursor_shape = settings.cursor_shape;
        let alternate_scroll = settings.alternate_scroll;
        let max_scroll_history_lines = settings.max_scroll_history_lines;
        let path_style = PathStyle::local();
        let window_id = window.window_handle().window_id().as_u64();
        let pane = self.pane.clone();
        let weak_workspace = self.workspace.clone();
        let this = cx.entity().downgrade();

        let terminal_task = TerminalBuilder::new_with_ssh(
            config,
            cursor_shape,
            alternate_scroll,
            max_scroll_history_lines,
            window_id,
            cx,
            path_style,
        );

        let task = cx.spawn_in(window, async move |_, cx| {
            let terminal_builder = match terminal_task.await {
                Ok(builder) => builder,
                Err(error) => {
                    log::error!("Failed to create SSH terminal: {}", error);
                    let error_message = format_ssh_error(&error);
                    this.update(cx, |this, cx| {
                        this.connection_status = ConnectionStatus::Error(error_message.into());
                        this.editor.update(cx, |editor, cx| {
                            editor.set_read_only(false);
                            cx.notify();
                        });
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };

            workspace
                .update_in(cx, |workspace, window, cx| {
                    let terminal_handle = cx.new(|cx| terminal_builder.subscribe(cx));
                    let terminal_view = Box::new(cx.new(|cx| {
                        TerminalView::new(
                            terminal_handle,
                            weak_workspace.clone(),
                            workspace.database_id(),
                            workspace.project().downgrade(),
                            window,
                            cx,
                        )
                    }));

                    pane.update(cx, |pane, cx| {
                        pane.add_item(terminal_view, true, true, None, window, cx);
                    });
                })
                .ok();

            this.update(cx, |_, cx| {
                cx.emit(DismissEvent);
            })
            .ok();
        });

        self._connecting_task = Some(task);
    }
}

fn format_ssh_error(error: &anyhow::Error) -> String {
    let error_string = format!("{:#}", error);

    if error_string.contains("authentication") {
        return t("ssh_connect.error_auth_failed").to_string();
    }

    if error_string.contains("Connection refused") {
        return t("ssh_connect.error_connection_refused").to_string();
    }

    if error_string.contains("No route to host") || error_string.contains("Network is unreachable")
    {
        return t("ssh_connect.error_network_unreachable").to_string();
    }

    if error_string.contains("timed out") || error_string.contains("Timeout") {
        return t("ssh_connect.error_timeout").to_string();
    }

    if error_string.contains("Name or service not known")
        || error_string.contains("Could not resolve")
    {
        return t("ssh_connect.error_hostname").to_string();
    }

    let root_cause = error.root_cause().to_string();
    if root_cause.len() <= 80 {
        return root_cause;
    }

    root_cause.chars().take(77).collect::<String>() + "..."
}

impl ModalView for SshConnectModal {}

impl EventEmitter<DismissEvent> for SshConnectModal {}

impl Focusable for SshConnectModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for SshConnectModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let status = self.connection_status.clone();

        v_flex()
            .key_context("SshConnectModal")
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .elevation_3(cx)
            .w_96()
            .overflow_hidden()
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.colors().border_variant)
                    .child(self.editor.clone()),
            )
            .child(
                h_flex()
                    .bg(theme.colors().editor_background)
                    .rounded_b_sm()
                    .w_full()
                    .p_2()
                    .gap_1()
                    .map(|this| match &status {
                        ConnectionStatus::Idle => this.child(
                            Label::new(t("ssh_connect.enter_string"))
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                        ),
                        ConnectionStatus::Connecting => this
                            .child(SpinnerLabel::new().size(LabelSize::Small))
                            .child(
                                Label::new(t("ssh_connect.connecting"))
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            ),
                        ConnectionStatus::Error(err) => this.child(
                            div().max_w_full().overflow_hidden().child(
                                Label::new(err.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Error),
                            ),
                        ),
                    }),
            )
    }
}

fn parse_ssh_string(input: &str) -> Result<SshConfig, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(t("ssh_connect.error_string_required").into());
    }

    // Try space-separated format: host username password [port]
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 3 && !input.contains('@') {
        let host = parts[0];
        let username = parts[1];
        let password = parts[2];
        let port = if parts.len() >= 4 {
            parts[3].parse::<u16>().map_err(|_| t("ssh_connect.error_invalid_port"))?
        } else {
            22
        };

        return Ok(SshConfig::new(host, port)
            .with_username(username)
            .with_auth(SshAuthConfig::Password(password.to_string())));
    }

    // Fall back to original format: user@host[:port]
    let (user_host, port) = if let Some((left, port_str)) = input.rsplit_once(':') {
        let port = port_str
            .parse::<u16>()
            .map_err(|_| t("ssh_connect.error_invalid_port"))?;
        (left, port)
    } else {
        (input, 22)
    };

    let (username, host) = user_host
        .split_once('@')
        .ok_or_else(|| t("ssh_connect.error_format_hint"))?;

    if username.is_empty() || host.is_empty() {
        return Err(t("ssh_connect.error_user_host_required").into());
    }

    Ok(SshConfig::new(host, port).with_username(username))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_string_basic() {
        let config = parse_ssh_string("root@192.168.1.100").unwrap();
        assert_eq!(config.host, "192.168.1.100");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, Some("root".to_string()));
    }

    #[test]
    fn test_parse_ssh_string_with_port() {
        let config = parse_ssh_string("admin@example.com:2222").unwrap();
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 2222);
        assert_eq!(config.username, Some("admin".to_string()));
    }

    #[test]
    fn test_parse_ssh_string_empty() {
        let result = parse_ssh_string("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Connection string required");
    }

    #[test]
    fn test_parse_ssh_string_no_at() {
        let result = parse_ssh_string("hostname");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Format: user@host[:port] or host user password [port]");
    }

    #[test]
    fn test_parse_ssh_string_invalid_port() {
        let result = parse_ssh_string("user@host:notaport");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid port number");
    }

    #[test]
    fn test_parse_ssh_string_empty_username() {
        let result = parse_ssh_string("@host");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Username and host required");
    }

    #[test]
    fn test_parse_ssh_string_empty_host() {
        let result = parse_ssh_string("user@");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Username and host required");
    }

    #[test]
    fn test_parse_ssh_string_whitespace() {
        let config = parse_ssh_string("  user@host  ").unwrap();
        assert_eq!(config.host, "host");
        assert_eq!(config.username, Some("user".to_string()));
    }

    #[test]
    fn test_parse_ssh_string_space_format_basic() {
        let config = parse_ssh_string("127.0.0.1 root root").unwrap();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, Some("root".to_string()));
        assert!(matches!(config.auth, SshAuthConfig::Password(ref p) if p == "root"));
    }

    #[test]
    fn test_parse_ssh_string_space_format_with_port() {
        let config = parse_ssh_string("192.168.1.1 admin password123 2222").unwrap();
        assert_eq!(config.host, "192.168.1.1");
        assert_eq!(config.port, 2222);
        assert_eq!(config.username, Some("admin".to_string()));
        assert!(matches!(config.auth, SshAuthConfig::Password(ref p) if p == "password123"));
    }

    #[test]
    fn test_parse_ssh_string_space_format_invalid_port() {
        let result = parse_ssh_string("host user pass notaport");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid port number");
    }
}
