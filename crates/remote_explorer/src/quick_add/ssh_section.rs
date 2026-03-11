use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window};
use i18n::t;
use ui::{
    prelude::*, Button, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, h_flex,
    v_flex,
};

pub struct SshSection {
    host_editor: Entity<Editor>,
    username_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
}

impl SshSection {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let host_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("ssh_connect.placeholder"), window, cx);
            editor
        });

        let username_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("session_edit.username"), window, cx);
            editor.set_text("root", window, cx);
            editor
        });

        let password_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("session_edit.password"), window, cx);
            editor.set_text("root", window, cx);
            editor
        });

        Self {
            host_editor,
            username_editor,
            password_editor,
        }
    }

    pub fn get_values(&self, cx: &App) -> (String, u16, String, String) {
        let host_input = self.host_editor.read(cx).text(cx);
        let (host, port, parsed_username) = parse_ssh_host_string(&host_input);
        let username = {
            let editor_value = self.username_editor.read(cx).text(cx);
            if let Some(parsed) = parsed_username {
                parsed
            } else if editor_value.is_empty() {
                "root".to_string()
            } else {
                editor_value
            }
        };
        let password = {
            let editor_value = self.password_editor.read(cx).text(cx);
            if editor_value.is_empty() {
                "root".to_string()
            } else {
                editor_value
            }
        };
        (host, port, username, password)
    }

    pub fn clear_fields(&mut self, window: &mut Window, cx: &mut App) {
        self.host_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.username_editor.update(cx, |editor, cx| {
            editor.set_text("root", window, cx);
        });
        self.password_editor.update(cx, |editor, cx| {
            editor.set_text("root", window, cx);
        });
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.host_editor
    }

    pub fn username_editor(&self) -> &Entity<Editor> {
        &self.username_editor
    }

    pub fn password_editor(&self) -> &Entity<Editor> {
        &self.password_editor
    }

    pub fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.colors().border;

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Server)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(t("remote_explorer.ssh_quick_connect"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.host_editor.clone()),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.username_editor.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.password_editor.clone()),
                    )
                    .child(
                        Button::new("ssh-connect", t("common.connect"))
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact),
                    ),
            )
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
