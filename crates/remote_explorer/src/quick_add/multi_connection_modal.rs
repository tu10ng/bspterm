use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window,
};
use i18n::t;
use terminal::SessionStoreEntity;
use ui::{prelude::*, Button, ButtonStyle, Color, Label, LabelSize, h_flex, v_flex};
use workspace::{ModalView, Pane};

use super::{ConnectionProtocol, ParsedConnection};

struct ConnectionEntry {
    host: String,
    protocol: ConnectionProtocol,
    port_editor: Entity<Editor>,
    username_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
}

pub struct MultiConnectionModal {
    connections: Vec<ConnectionEntry>,
    apply_all_username_editor: Entity<Editor>,
    apply_all_password_editor: Entity<Editor>,
    session_store: Entity<SessionStoreEntity>,
    #[allow(dead_code)]
    pane: Option<Entity<Pane>>,
    focus_handle: FocusHandle,
}

impl MultiConnectionModal {
    pub fn new(
        parsed: Vec<ParsedConnection>,
        session_store: Entity<SessionStoreEntity>,
        pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let connections = parsed
            .into_iter()
            .map(|conn| {
                let port_editor = cx.new(|cx| {
                    let mut editor = Editor::single_line(window, cx);
                    editor.set_text(conn.port.to_string(), window, cx);
                    editor
                });

                let username_editor = cx.new(|cx| {
                    let mut editor = Editor::single_line(window, cx);
                    if let Some(username) = conn.username.clone() {
                        editor.set_text(username, window, cx);
                    }
                    editor
                });

                let password_editor = cx.new(|cx| {
                    let mut editor = Editor::single_line(window, cx);
                    if let Some(password) = conn.password.clone() {
                        editor.set_text(password, window, cx);
                    }
                    editor
                });

                ConnectionEntry {
                    host: conn.host,
                    protocol: conn.protocol,
                    port_editor,
                    username_editor,
                    password_editor,
                }
            })
            .collect();

        let apply_all_username_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("session_edit.username"), window, cx);
            editor
        });

        let apply_all_password_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("session_edit.password"), window, cx);
            editor
        });

        Self {
            connections,
            apply_all_username_editor,
            apply_all_password_editor,
            session_store,
            pane,
            focus_handle: cx.focus_handle(),
        }
    }

    fn apply_to_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let username = self.apply_all_username_editor.read(cx).text(cx);
        let password = self.apply_all_password_editor.read(cx).text(cx);

        for entry in &self.connections {
            if !username.is_empty() {
                let username_clone = username.clone();
                entry.username_editor.update(cx, |editor, cx| {
                    editor.set_text(username_clone, window, cx);
                });
            }
            if !password.is_empty() {
                let password_clone = password.clone();
                entry.password_editor.update(cx, |editor, cx| {
                    editor.set_text(password_clone, window, cx);
                });
            }
        }
    }

    fn connect_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        for entry in &self.connections {
            let port = entry
                .port_editor
                .read(cx)
                .text(cx)
                .parse::<u16>()
                .unwrap_or(23);
            let username = entry.username_editor.read(cx).text(cx);
            let password = entry.password_editor.read(cx).text(cx);

            match entry.protocol {
                ConnectionProtocol::Telnet => {
                    let config = terminal::TelnetSessionConfig::new(&entry.host, port);
                    let config = if !username.is_empty() {
                        config.with_credentials(username.clone(), password.clone())
                    } else {
                        config
                    };

                    let session_name = if username.is_empty() {
                        format!("{}:{}", entry.host, port)
                    } else {
                        format!("{}@{}:{}", username, entry.host, port)
                    };

                    let session_config =
                        terminal::SessionConfig::new_telnet(session_name.clone(), config);
                    self.session_store.update(cx, |store, cx| {
                        store.add_session(session_config, None, cx);
                    });

                    log::info!("Telnet connection not yet implemented: {}", session_name);
                }
                ConnectionProtocol::Ssh => {
                    let username = if username.is_empty() {
                        "root".to_string()
                    } else {
                        username.clone()
                    };
                    let password = if password.is_empty() {
                        "root".to_string()
                    } else {
                        password.clone()
                    };

                    let ssh_config = terminal::SshSessionConfig::new(&entry.host, port)
                        .with_username(&username)
                        .with_auth(terminal::AuthMethod::Password { password });

                    let session_name = format!("{}@{}:{}", username, entry.host, port);
                    let session_config =
                        terminal::SessionConfig::new_ssh(session_name, ssh_config);

                    self.session_store.update(cx, |store, cx| {
                        store.add_session(session_config, None, cx);
                    });
                }
            }
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn render_all_connection_rows(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Div> {
        let theme = cx.theme();
        let border_color = theme.colors().border;

        self.connections
            .iter()
            .map(|entry| {
                let protocol_label = match entry.protocol {
                    ConnectionProtocol::Telnet => "Telnet",
                    ConnectionProtocol::Ssh => "SSH",
                };

                h_flex()
                    .w_full()
                    .gap_1()
                    .py_1()
                    .child(
                        div()
                            .w_32()
                            .overflow_hidden()
                            .child(Label::new(entry.host.clone()).size(LabelSize::Small)),
                    )
                    .child(
                        div()
                            .w_20()
                            .child(Label::new(protocol_label).size(LabelSize::Small)),
                    )
                    .child(
                        div()
                            .w_16()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(entry.port_editor.clone()),
                    )
                    .child(
                        div()
                            .w_24()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(entry.username_editor.clone()),
                    )
                    .child(
                        div()
                            .w_24()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(entry.password_editor.clone()),
                    )
            })
            .collect()
    }
}

impl ModalView for MultiConnectionModal {}

impl EventEmitter<DismissEvent> for MultiConnectionModal {}

impl Focusable for MultiConnectionModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultiConnectionModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connection_rows = self.render_all_connection_rows(window, cx);
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let border_variant_color = theme.colors().border_variant;
        let editor_bg = theme.colors().editor_background;

        v_flex()
            .key_context("MultiConnectionModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w_128()
            .max_h_96()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .justify_between()
                    .child(Label::new(t("multi_connection.title")))
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
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .bg(editor_bg)
                    .child(div().w_32().child(Label::new(t("multi_connection.ip")).size(LabelSize::XSmall).color(Color::Muted)))
                    .child(div().w_20().child(Label::new(t("multi_connection.protocol")).size(LabelSize::XSmall).color(Color::Muted)))
                    .child(div().w_16().child(Label::new(t("multi_connection.port")).size(LabelSize::XSmall).color(Color::Muted)))
                    .child(div().w_24().child(Label::new(t("multi_connection.user")).size(LabelSize::XSmall).color(Color::Muted)))
                    .child(div().w_24().child(Label::new(t("multi_connection.pass")).size(LabelSize::XSmall).color(Color::Muted))),
            )
            .child(
                v_flex()
                    .id("connections-list")
                    .w_full()
                    .px_2()
                    .overflow_y_scroll()
                    .max_h_64()
                    .children(connection_rows),
            )
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(border_variant_color)
                    .bg(editor_bg)
                    .child(Label::new(t("multi_connection.apply_to_all")).size(LabelSize::Small).color(Color::Muted))
                    .child(
                        div()
                            .w_24()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.apply_all_username_editor.clone()),
                    )
                    .child(
                        div()
                            .w_24()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.apply_all_password_editor.clone()),
                    )
                    .child(
                        Button::new("apply-all", t("multi_connection.apply"))
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_to_all(window, cx);
                            })),
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
                        Button::new("connect-all", t("multi_connection.connect_all"))
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.connect_all(window, cx);
                            })),
                    ),
            )
    }
}
