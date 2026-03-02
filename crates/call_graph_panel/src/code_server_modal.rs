use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Window, px,
};
use log_tracer::CodeServerConfig;
use ui::{prelude::*, Button, ButtonCommon, ButtonStyle, Checkbox, Label, LabelSize, h_flex, v_flex};
use workspace::ModalView;

pub struct CodeServerModal {
    ssh_host_editor: Entity<Editor>,
    ssh_port_editor: Entity<Editor>,
    ssh_user_editor: Entity<Editor>,
    ssh_password_editor: Entity<Editor>,
    container_id_editor: Entity<Editor>,
    code_root_editor: Entity<Editor>,
    save_password: bool,
    focus_handle: FocusHandle,
    on_save: Option<Box<dyn FnOnce(CodeServerConfig) + Send + 'static>>,
}

impl CodeServerModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = CodeServerConfig::load().unwrap_or_default();
        Self::with_config(config, window, cx)
    }

    pub fn with_config(config: CodeServerConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let ssh_host_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("e.g., 192.168.1.100", window, cx);
            if !config.ssh_host.is_empty() {
                editor.set_text(config.ssh_host.clone(), window, cx);
            }
            editor
        });

        let ssh_port_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(config.ssh_port.to_string(), window, cx);
            editor
        });

        let ssh_user_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("e.g., root", window, cx);
            if !config.ssh_user.is_empty() {
                editor.set_text(config.ssh_user.clone(), window, cx);
            }
            editor
        });

        let ssh_password_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Password (optional)", window, cx);
            if let Some(ref password) = config.ssh_password {
                editor.set_text(password.clone(), window, cx);
            }
            editor
        });

        let container_id_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Leave empty if code is on server", window, cx);
            if !config.container_id.is_empty() {
                editor.set_text(config.container_id.clone(), window, cx);
            }
            editor
        });

        let code_root_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("e.g., /usr1", window, cx);
            if !config.code_root.is_empty() {
                editor.set_text(config.code_root.clone(), window, cx);
            }
            editor
        });

        let save_password = config.ssh_password.is_some();

        Self {
            ssh_host_editor,
            ssh_port_editor,
            ssh_user_editor,
            ssh_password_editor,
            container_id_editor,
            code_root_editor,
            save_password,
            focus_handle,
            on_save: None,
        }
    }

    pub fn on_save<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(CodeServerConfig) + Send + 'static,
    {
        self.on_save = Some(Box::new(callback));
        self
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let ssh_host = self.ssh_host_editor.read(cx).text(cx).trim().to_string();
        let ssh_port_str = self.ssh_port_editor.read(cx).text(cx);
        let ssh_port: u16 = ssh_port_str.trim().parse().unwrap_or(22);
        let ssh_user = self.ssh_user_editor.read(cx).text(cx).trim().to_string();
        let ssh_password = {
            let text = self.ssh_password_editor.read(cx).text(cx);
            if self.save_password && !text.trim().is_empty() {
                Some(text.trim().to_string())
            } else {
                None
            }
        };
        let container_id = self.container_id_editor.read(cx).text(cx).trim().to_string();
        let code_root = self.code_root_editor.read(cx).text(cx).trim().to_string();

        let config = CodeServerConfig {
            ssh_host,
            ssh_port,
            ssh_user,
            ssh_password,
            container_id,
            code_root,
        };

        if let Err(err) = config.save() {
            log::error!("[CodeServerModal] Failed to save config: {}", err);
        } else {
            log::info!("[CodeServerModal] Config saved successfully");
        }

        if let Some(callback) = self.on_save.take() {
            callback(config);
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn toggle_save_password(&mut self, cx: &mut Context<Self>) {
        self.save_password = !self.save_password;
        cx.notify();
    }
}

impl Render for CodeServerModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let save_password = self.save_password;

        v_flex()
            .id("code-server-modal")
            .key_context("CodeServerModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(450.))
            .child(Label::new("Code Server Configuration").size(LabelSize::Large))
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("SSH Host").size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.ssh_host_editor.clone()),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(Label::new("SSH Port").size(LabelSize::Small))
                            .child(
                                div()
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .rounded_md()
                                    .px_2()
                                    .py_1()
                                    .child(self.ssh_port_editor.clone()),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(Label::new("Username").size(LabelSize::Small))
                            .child(
                                div()
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .rounded_md()
                                    .px_2()
                                    .py_1()
                                    .child(self.ssh_user_editor.clone()),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Password").size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.ssh_password_editor.clone()),
                    )
                    .child(
                        Checkbox::new("save-password", save_password.into())
                            .label("Save password")
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_save_password(cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Docker Container (optional)").size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.container_id_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Code Root Path").size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.code_root_editor.clone()),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .mt_2()
                    .child(
                        Button::new("cancel", "Cancel")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("save", "Save")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}

impl Focusable for CodeServerModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for CodeServerModal {}

impl ModalView for CodeServerModal {}
