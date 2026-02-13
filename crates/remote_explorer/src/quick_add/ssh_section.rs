use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window};
use ui::{
    prelude::*, Button, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, h_flex,
    v_flex,
};

pub struct SshSection {
    host_editor: Entity<Editor>,
}

impl SshSection {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let host_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("user@host:port", window, cx);
            editor
        });

        Self { host_editor }
    }

    pub fn get_host(&self, cx: &App) -> String {
        self.host_editor.read(cx).text(cx)
    }

    pub fn clear_host(&mut self, window: &mut Window, cx: &mut App) {
        self.host_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.host_editor
    }

    pub fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

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
                        Label::new("SSH Quick Connect")
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
                            .border_color(theme.colors().border)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.host_editor.clone()),
                    )
                    .child(
                        Button::new("ssh-connect", "Connect")
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact),
                    ),
            )
            .child(
                Label::new("Default: root/root")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }
}
