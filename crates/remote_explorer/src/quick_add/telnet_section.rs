use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Subscription, Window};
use terminal::SessionStoreEntity;
use ui::{
    prelude::*, Button, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, h_flex,
    v_flex,
};
use uuid::Uuid;

pub struct TelnetSection {
    pub ip_editor: Entity<Editor>,
    pub port_editor: Entity<Editor>,
    pub username_editor: Entity<Editor>,
    pub password_editor: Entity<Editor>,
    selected_preset: Option<Uuid>,
    session_store: Entity<SessionStoreEntity>,
    _subscriptions: Vec<Subscription>,
}

impl TelnetSection {
    pub fn new(
        session_store: Entity<SessionStoreEntity>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let ip_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("IP address", window, cx);
            editor
        });

        let port_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("23", window, cx);
            editor
        });

        let username_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Username", window, cx);
            editor
        });

        let password_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Password", window, cx);
            editor
        });

        Self {
            ip_editor,
            port_editor,
            username_editor,
            password_editor,
            selected_preset: None,
            session_store,
            _subscriptions: Vec::new(),
        }
    }

    pub fn get_values(&self, cx: &App) -> (String, String, String, String) {
        let ip = self.ip_editor.read(cx).text(cx);
        let port = self.port_editor.read(cx).text(cx);
        let port = if port.is_empty() { "23".to_string() } else { port };
        let username = self.username_editor.read(cx).text(cx);
        let password = self.password_editor.read(cx).text(cx);
        (ip, port, username, password)
    }

    pub fn clear_fields(&mut self, window: &mut Window, cx: &mut App) {
        self.ip_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.port_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.username_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.password_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.selected_preset = None;
    }

    pub fn select_preset(&mut self, preset_id: Option<Uuid>, window: &mut Window, cx: &mut App) {
        self.selected_preset = preset_id;

        if let Some(id) = preset_id {
            let session_store = self.session_store.read(cx);
            if let Some(preset) = session_store.credential_presets().iter().find(|p| p.id == id) {
                let username = preset.username.clone();
                let password = preset.password.clone();

                self.username_editor.update(cx, |editor, cx| {
                    editor.set_text(username, window, cx);
                });
                self.password_editor.update(cx, |editor, cx| {
                    editor.set_text(password, window, cx);
                });
            }
        }
    }

    pub fn clear_preset_selection(&mut self) {
        self.selected_preset = None;
    }

    pub fn get_preset_label(&self, cx: &App) -> String {
        match self.selected_preset {
            None => "Custom".to_string(),
            Some(id) => {
                let session_store = self.session_store.read(cx);
                session_store
                    .credential_presets()
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Custom".to_string())
            }
        }
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
                        Icon::new(IconName::Terminal)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new("Telnet Quick Connect")
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
                            .child(self.ip_editor.clone()),
                    )
                    .child(
                        div()
                            .w_16()
                            .border_1()
                            .border_color(border_color)
                            .rounded_sm()
                            .px_1()
                            .py_px()
                            .child(self.port_editor.clone()),
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
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .child(
                        Button::new("telnet-connect", "Connect")
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact),
                    ),
            )
    }
}
