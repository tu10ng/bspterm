use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};
use i18n::t;
use terminal::{
    AuthMethod, ProtocolConfig, SessionConfig, SessionNode, SessionStoreEntity,
    SshSessionConfig, TelnetSessionConfig,
};
use ui::{
    prelude::*, Button, ButtonStyle, Color, ContextMenu, DropdownMenu, DropdownStyle, Label,
    LabelSize, h_flex, v_flex,
};
use uuid::Uuid;
use workspace::ModalView;

pub struct SessionEditModal {
    session_id: Uuid,
    session_store: Entity<SessionStoreEntity>,
    name_editor: Entity<Editor>,
    host_editor: Entity<Editor>,
    port_editor: Entity<Editor>,
    username_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
    selected_terminal_type: Option<String>,
    selected_credential: Option<(String, String)>,
    programmatic_change_count: usize,
    protocol: ProtocolType,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq)]
enum ProtocolType {
    Ssh,
    Telnet,
}

impl SessionEditModal {
    pub fn new(session_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session_store = SessionStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let (name, host, port, username, password, terminal_type, protocol) = {
            let store = session_store.read(cx);
            if let Some(SessionNode::Session(session)) = store.store().find_node(session_id) {
                extract_session_data(session)
            } else {
                (
                    String::new(),
                    String::new(),
                    22,
                    String::new(),
                    String::new(),
                    None,
                    ProtocolType::Ssh,
                )
            }
        };

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(name, window, cx);
            editor.set_placeholder_text(&t("session_edit.session_name"), window, cx);
            editor
        });

        let host_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(host, window, cx);
            editor.set_placeholder_text(&t("session_edit.host"), window, cx);
            editor
        });

        let port_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(port.to_string(), window, cx);
            editor.set_placeholder_text(&t("session_edit.port"), window, cx);
            editor
        });

        let username_for_cred = username.clone();
        let password_for_cred = password.clone();

        let username_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(username, window, cx);
            editor.set_placeholder_text(&t("session_edit.username"), window, cx);
            editor
        });

        let password_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(password, window, cx);
            editor.set_placeholder_text(&t("session_edit.password"), window, cx);
            editor
        });

        let selected_credential = if protocol == ProtocolType::Telnet
            && !username_for_cred.is_empty()
            && !password_for_cred.is_empty()
        {
            let credentials = session_store.read(cx).store().collect_telnet_credentials();
            if credentials
                .iter()
                .any(|(u, p)| u == &username_for_cred && p == &password_for_cred)
            {
                Some((username_for_cred, password_for_cred))
            } else {
                None
            }
        } else {
            None
        };

        let username_subscription =
            cx.subscribe(&username_editor, |this: &mut Self, _, event: &editor::EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::BufferEdited { .. }) {
                    if this.programmatic_change_count > 0 {
                        this.programmatic_change_count -= 1;
                    } else {
                        this.selected_credential = None;
                        cx.notify();
                    }
                }
            });

        let password_subscription =
            cx.subscribe(&password_editor, |this: &mut Self, _, event: &editor::EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::BufferEdited { .. }) {
                    if this.programmatic_change_count > 0 {
                        this.programmatic_change_count -= 1;
                    } else {
                        this.selected_credential = None;
                        cx.notify();
                    }
                }
            });

        Self {
            session_id,
            session_store,
            name_editor,
            host_editor,
            port_editor,
            username_editor,
            password_editor,
            selected_terminal_type: terminal_type,
            selected_credential,
            programmatic_change_count: 0,
            protocol,
            focus_handle,
            _subscriptions: vec![username_subscription, password_subscription],
        }
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);
        let host = self.host_editor.read(cx).text(cx);
        let port = self
            .port_editor
            .read(cx)
            .text(cx)
            .parse::<u16>()
            .unwrap_or(if self.protocol == ProtocolType::Ssh { 22 } else { 23 });
        let username = self.username_editor.read(cx).text(cx);
        let password = self.password_editor.read(cx).text(cx);
        let terminal_type = self.selected_terminal_type.clone();

        let protocol = self.protocol;
        self.session_store.update(cx, |store, cx| {
            store.update_session(
                self.session_id,
                |session| {
                    session.name = name;
                    match protocol {
                        ProtocolType::Ssh => {
                            session.protocol = ProtocolConfig::Ssh(SshSessionConfig {
                                host,
                                port,
                                username: if username.is_empty() {
                                    None
                                } else {
                                    Some(username)
                                },
                                auth: if password.is_empty() {
                                    AuthMethod::Interactive
                                } else {
                                    AuthMethod::Password { password }
                                },
                                env: std::collections::HashMap::new(),
                                keepalive_interval_secs: Some(30),
                                initial_command: None,
                                terminal_type,
                            });
                        }
                        ProtocolType::Telnet => {
                            session.protocol = ProtocolConfig::Telnet(TelnetSessionConfig {
                                host,
                                port,
                                username: if username.is_empty() {
                                    None
                                } else {
                                    Some(username)
                                },
                                password: if password.is_empty() {
                                    None
                                } else {
                                    Some(password)
                                },
                                encoding: None,
                                terminal_type,
                            });
                        }
                    }
                },
                cx,
            );
        });

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn select_credential(
        &mut self,
        credential: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_credential = credential.clone();
        if let Some((username, password)) = credential {
            self.programmatic_change_count += 2;
            self.username_editor.update(cx, |editor, cx| {
                editor.set_text(username, window, cx);
            });
            self.password_editor.update(cx, |editor, cx| {
                editor.set_text(password, window, cx);
            });
        }
        cx.notify();
    }

    fn get_credential_label(&self) -> String {
        match &self.selected_credential {
            None => t("common.custom").to_string(),
            Some((username, password)) => format!("{}/{}", username, password),
        }
    }
}

fn extract_session_data(
    session: &SessionConfig,
) -> (String, String, u16, String, String, Option<String>, ProtocolType) {
    match &session.protocol {
        ProtocolConfig::Ssh(ssh) => {
            let password = match &ssh.auth {
                AuthMethod::Password { password } => password.clone(),
                _ => String::new(),
            };
            (
                session.name.clone(),
                ssh.host.clone(),
                ssh.port,
                ssh.username.clone().unwrap_or_default(),
                password,
                ssh.terminal_type.clone(),
                ProtocolType::Ssh,
            )
        }
        ProtocolConfig::Telnet(telnet) => (
            session.name.clone(),
            telnet.host.clone(),
            telnet.port,
            telnet.username.clone().unwrap_or_default(),
            telnet.password.clone().unwrap_or_default(),
            telnet.terminal_type.clone(),
            ProtocolType::Telnet,
        ),
    }
}

impl ModalView for SessionEditModal {}

impl EventEmitter<DismissEvent> for SessionEditModal {}

impl Focusable for SessionEditModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionEditModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;
        let border_variant_color = theme.colors().border_variant;

        let protocol_label = match self.protocol {
            ProtocolType::Ssh => "SSH",
            ProtocolType::Telnet => "Telnet",
        };

        let terminal_type_label = self
            .selected_terminal_type
            .clone()
            .unwrap_or_else(|| "xterm-256color".to_string());

        let weak_self = cx.weak_entity();
        let terminal_type_menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for term_type in ["xterm-256color", "xterm", "vt100", "linux", "dumb"] {
                let label = term_type.to_string();
                let term_type_owned = term_type.to_string();
                let weak = weak_self.clone();
                menu = menu.entry(label, None, move |_, cx| {
                    weak.update(cx, |this, cx| {
                        this.selected_terminal_type = if term_type_owned == "xterm-256color" {
                            None
                        } else {
                            Some(term_type_owned.clone())
                        };
                        cx.notify();
                    })
                    .ok();
                });
            }
            menu
        });

        let credential_dropdown = if self.protocol == ProtocolType::Telnet {
            let credentials = self.session_store.read(cx).store().collect_telnet_credentials();
            let credential_label = self.get_credential_label();
            let weak_self = cx.weak_entity();
            let credential_menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
                let weak_for_custom = weak_self.clone();
                menu = menu.entry("Custom", None, move |window, cx| {
                    weak_for_custom
                        .update(cx, |this, cx| {
                            this.select_credential(None, window, cx);
                        })
                        .ok();
                });
                for (username, password) in &credentials {
                    let label = format!("{}/{}", username, password);
                    let credential = (username.clone(), password.clone());
                    let weak = weak_self.clone();
                    menu = menu.entry(label, None, move |window, cx| {
                        let cred = credential.clone();
                        weak.update(cx, |this, cx| {
                            this.select_credential(Some(cred), window, cx);
                        })
                        .ok();
                    });
                }
                menu
            });
            Some((credential_label, credential_menu))
        } else {
            None
        };

        v_flex()
            .key_context("SessionEditModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w_80()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .justify_between()
                    .child(Label::new(t("session_edit.title_edit_protocol").replace("{}", protocol_label)))
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
                    .w_full()
                    .p_2()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("common.name")).size(LabelSize::Small).color(Color::Muted))
                            .child(
                                div()
                                    .w_full()
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_sm()
                                    .px_1()
                                    .py_px()
                                    .child(self.name_editor.clone()),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(
                                        Label::new(t("session_edit.host"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .border_1()
                                            .border_color(border_color)
                                            .rounded_sm()
                                            .px_1()
                                            .py_px()
                                            .child(self.host_editor.clone()),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .w_16()
                                    .gap_1()
                                    .child(
                                        Label::new(t("session_edit.port"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .border_1()
                                            .border_color(border_color)
                                            .rounded_sm()
                                            .px_1()
                                            .py_px()
                                            .child(self.port_editor.clone()),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(
                                        Label::new(t("session_edit.username"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .border_1()
                                            .border_color(border_color)
                                            .rounded_sm()
                                            .px_1()
                                            .py_px()
                                            .child(self.username_editor.clone()),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(
                                        Label::new(t("session_edit.password"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .border_1()
                                            .border_color(border_color)
                                            .rounded_sm()
                                            .px_1()
                                            .py_px()
                                            .child(self.password_editor.clone()),
                                    ),
                            ),
                    )
                    .when_some(
                        credential_dropdown,
                        |this, (credential_label, credential_menu)| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Label::new(t("remote_explorer.credential"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        DropdownMenu::new(
                                            "credential",
                                            credential_label,
                                            credential_menu,
                                        )
                                        .trigger_size(ButtonSize::Compact),
                                    ),
                            )
                        },
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                Label::new(t("session_edit.terminal_type"))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                DropdownMenu::new(
                                    "terminal-type",
                                    terminal_type_label,
                                    terminal_type_menu,
                                )
                                .full_width(true)
                                .style(DropdownStyle::Outlined),
                            ),
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
                        Button::new("save", t("common.save"))
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}
