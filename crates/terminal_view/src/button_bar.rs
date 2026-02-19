use editor::Editor;
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, Styled, Subscription, Task, WeakEntity, Window,
};
use language::Buffer;
use project::Project;
use std::path::Path;
use std::{fs, path::PathBuf};
use terminal::{ButtonBarStoreEntity, ButtonBarStoreEvent, ButtonConfig};
use uuid::Uuid;
use ui::{FluentBuilder, IconButton, IconName, IconSize, Label, Switch, ToggleState, prelude::*};
use workspace::{
    ModalView, SaveIntent, Workspace,
    item::{Item, ItemEvent, SaveOptions, TabContentParams},
};

use script_panel::script_runner::{ScriptRunner, ScriptStatus};

pub use script_panel::script_runner;

/// Button bar script runner state.
pub struct ButtonBarScriptRunner {
    runner: ScriptRunner,
}

impl ButtonBarScriptRunner {
    pub fn new(script_path: PathBuf, socket_path: PathBuf, terminal_id: Option<String>) -> Self {
        Self {
            runner: ScriptRunner::new(script_path, socket_path, terminal_id),
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        self.runner.start()
    }

    pub fn stop(&mut self) {
        self.runner.stop();
    }

    pub fn status(&mut self) -> &ScriptStatus {
        self.runner.status()
    }

    pub fn read_output(&mut self) -> Option<String> {
        self.runner.read_output()
    }
}

/// Event type for ButtonScriptEditor
#[allow(dead_code)]
pub enum ButtonScriptEditorEvent {
    Edited,
}

/// Special editor for editing button bar scripts.
/// Overrides save behavior: shows naming dialog instead of saving directly.
pub struct ButtonScriptEditor {
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
}

impl ButtonScriptEditor {
    pub fn new(
        buffer: Entity<Buffer>,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::for_buffer(buffer.clone(), Some(project), window, cx));
        let focus_handle = cx.focus_handle();
        Self {
            editor,
            buffer,
            workspace,
            focus_handle,
        }
    }
}

impl EventEmitter<ButtonScriptEditorEvent> for ButtonScriptEditor {}

impl Focusable for ButtonScriptEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ButtonScriptEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}

impl Item for ButtonScriptEditor {
    type Event = ButtonScriptEditorEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "新建按钮脚本".into()
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, _cx: &App) -> AnyElement {
        Label::new("新建按钮脚本")
            .color(params.text_color())
            .into_any_element()
    }

    fn to_item_events(event: &Self::Event, f: impl FnMut(ItemEvent)) {
        let mut f = f;
        match event {
            ButtonScriptEditorEvent::Edited => f(ItemEvent::Edit),
        }
    }

    fn can_save(&self, _cx: &App) -> bool {
        true
    }

    fn save(
        &mut self,
        _options: SaveOptions,
        _project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let content = self.buffer.read(cx).text();
        let editor_entity_id = cx.entity().entity_id();
        let workspace = self.workspace.clone();

        if let Some(ws) = workspace.upgrade() {
            ws.update(cx, |ws, cx| {
                ws.toggle_modal(window, cx, |window, cx| {
                    AddButtonModal::new(content, editor_entity_id, workspace.clone(), window, cx)
                });
            });
        }

        Task::ready(Ok(()))
    }
}

/// Information about a script file.
struct ScriptInfo {
    name: String,
    path: PathBuf,
}

/// Configuration modal for the button bar.
pub struct ButtonBarConfigModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    _subscription: Subscription,
}

impl ModalView for ButtonBarConfigModal {}

impl EventEmitter<DismissEvent> for ButtonBarConfigModal {}

impl ButtonBarConfigModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &ButtonBarStoreEntity::global(cx),
            |_this, _, _event: &ButtonBarStoreEvent, cx| {
                cx.notify();
            },
        );

        Self {
            focus_handle,
            workspace,
            _subscription: subscription,
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn scan_scripts(scripts_dir: &Path) -> Vec<ScriptInfo> {
        let mut scripts = Vec::new();
        if let Ok(entries) = std::fs::read_dir(scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.file_name().is_some_and(|n| n == "bspterm.py") {
                    continue;
                }
                if path.extension().is_some_and(|ext| ext == "py") {
                    if let Some(name) = path.file_stem() {
                        scripts.push(ScriptInfo {
                            name: name.to_string_lossy().to_string(),
                            path,
                        });
                    }
                }
            }
        }
        scripts.sort_by(|a, b| a.name.cmp(&b.name));
        scripts
    }

    fn toggle_script(script_path: PathBuf, enabled: bool, cx: &mut App) {
        let Some(store) = ButtonBarStoreEntity::try_global(cx) else {
            return;
        };

        let existing = store
            .read(cx)
            .buttons()
            .iter()
            .find(|b| b.script_path == script_path)
            .map(|b| b.id);

        store.update(cx, |store, cx| {
            if let Some(id) = existing {
                store.update_button(id, |b| b.enabled = enabled, cx);
            } else if enabled {
                let label = script_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let mut button = ButtonConfig::new(label, script_path);
                button.enabled = true;
                store.add_button(button, cx);
            }
        });
    }

    fn add_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();
        let language_registry = project.read(cx).languages().clone();
        let workspace_weak = self.workspace.clone();

        cx.spawn_in(window, async move |_, cx| {
            let python_lang = language_registry.language_for_name("Python").await.ok();

            let template = r#"from bspterm import current_terminal

term = current_terminal()
# 在此编写你的脚本逻辑
output = term.run("ls -la")
print(output)
"#;
            workspace.update_in(cx, |workspace, window, cx| {
                let buffer = project.update(cx, |project, cx| {
                    project.create_local_buffer(template, python_lang, true, cx)
                });

                let script_editor = cx.new(|cx| {
                    ButtonScriptEditor::new(
                        buffer,
                        project.clone(),
                        workspace_weak,
                        window,
                        cx,
                    )
                });

                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(script_editor), true, true, None, window, cx);
                });
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        cx.emit(DismissEvent);
    }
}

impl Focusable for ButtonBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ButtonBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = ButtonBarStoreEntity::global(cx);
        let buttons = store.read(cx).buttons().to_vec();

        let scripts_dir = paths::config_dir().join("scripts");
        let all_scripts = Self::scan_scripts(&scripts_dir);

        v_flex()
            .id("button-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(320.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| {
                this.dismiss(window, cx);
            }))
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("快捷按钮配置"),
                    )
                    .child(
                        IconButton::new("close-modal", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .when(all_scripts.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text_muted)
                                .child("脚本目录为空，点击下方按钮添加脚本"),
                        )
                    })
                    .children(all_scripts.iter().map(|script| {
                        let is_enabled = buttons
                            .iter()
                            .find(|b| b.script_path == script.path)
                            .map(|b| b.enabled)
                            .unwrap_or(false);
                        let script_path = script.path.clone();
                        let script_name = script.name.clone();

                        h_flex()
                            .py_1()
                            .px_2()
                            .rounded_sm()
                            .justify_between()
                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                            .child(div().text_sm().child(script_name.clone()))
                            .child(
                                Switch::new(
                                    SharedString::from(format!("script-switch-{}", script_name)),
                                    if is_enabled {
                                        ToggleState::Selected
                                    } else {
                                        ToggleState::Unselected
                                    },
                                )
                                .on_click(move |state, _window, cx| {
                                    let enabled = *state == ToggleState::Selected;
                                    Self::toggle_script(script_path.clone(), enabled, cx);
                                }),
                            )
                    })),
            )
            .child(
                h_flex().justify_end().child(
                    ui::Button::new("add-button", "添加按钮")
                        .style(ui::ButtonStyle::Filled)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_button(window, cx);
                        })),
                ),
            )
    }
}

/// Modal dialog for adding a new button to the button bar.
/// Accepts script content and saves it to a file when confirmed.
pub struct AddButtonModal {
    focus_handle: FocusHandle,
    name_editor: Entity<Editor>,
    script_content: String,
    editor_entity_id: EntityId,
    workspace: WeakEntity<Workspace>,
}

impl ModalView for AddButtonModal {}

impl EventEmitter<DismissEvent> for AddButtonModal {}

impl Focusable for AddButtonModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AddButtonModal {
    pub fn new(
        script_content: String,
        editor_entity_id: EntityId,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let name_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_placeholder_text("按钮名称", window, cx);
            ed
        });

        Self {
            focus_handle,
            name_editor,
            script_content,
            editor_entity_id,
            workspace,
        }
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.name_editor.read(cx).text(cx);
        if label.is_empty() {
            return;
        }

        let scripts_dir = paths::config_dir().join("scripts");
        if let Err(e) = fs::create_dir_all(&scripts_dir) {
            log::error!("Failed to create scripts directory: {}", e);
            cx.emit(DismissEvent);
            return;
        }

        let script_path = scripts_dir.join(format!("{}.py", label));
        if let Err(e) = fs::write(&script_path, &self.script_content) {
            log::error!("Failed to write script file: {}", e);
            cx.emit(DismissEvent);
            return;
        }

        if let Some(store) = ButtonBarStoreEntity::try_global(cx) {
            let button = ButtonConfig::new(label, script_path);
            store.update(cx, |store, cx| {
                store.add_button(button, cx);
            });
        }

        if let Some(workspace) = self.workspace.upgrade() {
            let editor_entity_id = self.editor_entity_id;
            workspace.update(cx, |workspace, cx| {
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.close_item_by_id(editor_entity_id, SaveIntent::Skip, window, cx)
                        .detach();
                });
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for AddButtonModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("AddButtonModal")
            .elevation_3(cx)
            .p_4()
            .gap_2()
            .w(px(300.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("添加按钮"),
            )
            .child(self.name_editor.clone())
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", "取消").on_click(
                            cx.listener(|this, _, window, cx| this.dismiss(&menu::Cancel, window, cx)),
                        ),
                    )
                    .child(
                        ui::Button::new("confirm", "确定")
                            .style(ui::ButtonStyle::Filled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.confirm(&menu::Confirm, window, cx)),
                            ),
                    ),
            )
    }
}

/// Modal dialog for renaming a button.
pub struct RenameButtonModal {
    focus_handle: FocusHandle,
    name_editor: Entity<Editor>,
    button_id: Uuid,
    original_label: String,
}

impl ModalView for RenameButtonModal {}
impl EventEmitter<DismissEvent> for RenameButtonModal {}

impl Focusable for RenameButtonModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl RenameButtonModal {
    pub fn new(button_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = ButtonBarStoreEntity::global(cx);
        let original_label = store
            .read(cx)
            .find_button(button_id)
            .map(|b| b.label.clone())
            .unwrap_or_default();

        let name_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(original_label.clone(), window, cx);
            ed
        });

        Self {
            focus_handle,
            name_editor,
            button_id,
            original_label,
        }
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let new_label = self.name_editor.read(cx).text(cx);
        if new_label.is_empty() || new_label == self.original_label {
            cx.emit(DismissEvent);
            return;
        }

        if let Some(store) = ButtonBarStoreEntity::try_global(cx) {
            let old_script_path = store
                .read(cx)
                .find_button(self.button_id)
                .map(|b| b.script_path.clone());

            if let Some(old_path) = old_script_path {
                let new_path = old_path.with_file_name(format!("{}.py", new_label));

                if let Err(e) = fs::rename(&old_path, &new_path) {
                    log::error!("Failed to rename script file: {}", e);
                    cx.emit(DismissEvent);
                    return;
                }

                let button_id = self.button_id;
                store.update(cx, |store, cx| {
                    store.update_button(
                        button_id,
                        |button| {
                            button.label = new_label;
                            button.script_path = new_path;
                        },
                        cx,
                    );
                });
            }
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for RenameButtonModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("RenameButtonModal")
            .elevation_3(cx)
            .p_4()
            .gap_2()
            .w(px(300.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("修改按钮名称"),
            )
            .child(self.name_editor.clone())
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", "取消").on_click(
                            cx.listener(|this, _, window, cx| this.dismiss(&menu::Cancel, window, cx)),
                        ),
                    )
                    .child(
                        ui::Button::new("confirm", "确定")
                            .style(ui::ButtonStyle::Filled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.confirm(&menu::Confirm, window, cx)),
                            ),
                    ),
            )
    }
}
