use std::fs;
use std::path::{Path, PathBuf};

use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, WeakEntity, Window,
};
use i18n::t;
use terminal::{
    FunctionConfig, FunctionKind, TerminalProtocol, FunctionStoreEntity, FunctionStoreEvent,
};
use ui::Tooltip;
use ui::{
    Color, FluentBuilder, IconButton, IconName, IconSize, Label, ListItem, ListItemSpacing,
    Switch, TintColor, ToggleState, prelude::*,
};
use uuid::Uuid;
use workspace::{ModalView, OpenOptions, Workspace};

/// Information about a script file.
struct ScriptInfo {
    name: String,
    path: PathBuf,
}

/// Configuration modal for the function bar.
pub struct FunctionBarConfigModal {
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ModalView for FunctionBarConfigModal {}

impl EventEmitter<DismissEvent> for FunctionBarConfigModal {}

impl FunctionBarConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &FunctionStoreEntity::global(cx),
            |_this, _, _event: &FunctionStoreEvent, cx| {
                cx.notify();
            },
        );

        Self {
            focus_handle,
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
        let Some(store) = FunctionStoreEntity::try_global(cx) else {
            return;
        };

        let existing = store
            .read(cx)
            .functions()
            .iter()
            .find(|f| f.script_path == script_path)
            .map(|f| f.id);

        store.update(cx, |store, cx| {
            if let Some(id) = existing {
                store.update_function(id, |func| func.enabled = enabled, cx);
            } else if enabled {
                let name = script_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let func = FunctionConfig::new(name, script_path);
                store.add_function(func, cx);
            }
        });
    }

    fn delete_function(func_id: Uuid, cx: &mut App) {
        let Some(store) = FunctionStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.remove_function(func_id, cx);
        });
    }
}

impl Focusable for FunctionBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FunctionBarConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = FunctionStoreEntity::global(cx);
        let functions = store.read(cx).functions().to_vec();
        let function_enabled = store.read(cx).function_enabled();
        let abbreviation_enabled = store.read(cx).abbreviation_enabled();

        let abbreviation_items: Vec<_> = functions
            .iter()
            .filter(|f| f.is_abbreviation())
            .cloned()
            .collect();

        let scripts_dir = paths::config_dir().join("scripts");
        let all_scripts = Self::scan_scripts(&scripts_dir);

        v_flex()
            .id("function-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(420.0))
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
                            .child(t("function.config_title")),
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
                h_flex()
                    .py_1()
                    .px_2()
                    .rounded_sm()
                    .justify_between()
                    .bg(cx.theme().colors().element_background)
                    .child(div().text_sm().child(t("function.enable_invocation")))
                    .child(
                        Switch::new(
                            "function-enabled-switch",
                            if function_enabled {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .on_click(move |_state, _window, cx| {
                            if let Some(store) = FunctionStoreEntity::try_global(cx) {
                                store.update(cx, |store, cx| {
                                    store.toggle_function_enabled(cx);
                                });
                            }
                        }),
                    ),
            )
            .child(
                h_flex()
                    .py_1()
                    .px_2()
                    .rounded_sm()
                    .justify_between()
                    .bg(cx.theme().colors().element_background)
                    .child(div().text_sm().child(t("function.enable_abbreviation")))
                    .child(
                        Switch::new(
                            "abbreviation-enabled-switch",
                            if abbreviation_enabled {
                                ToggleState::Selected
                            } else {
                                ToggleState::Unselected
                            },
                        )
                        .on_click(move |_state, _window, cx| {
                            if let Some(store) = FunctionStoreEntity::try_global(cx) {
                                store.update(cx, |store, cx| {
                                    store.toggle_abbreviation_enabled(cx);
                                });
                            }
                        }),
                    ),
            )
            .when(!abbreviation_items.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .child(t("function.abbr_label")),
                )
                .child(
                    v_flex().gap_1().children(abbreviation_items.iter().map(|func| {
                        let func_id = func.id;
                        let (trigger, expansion) = match &func.kind {
                            FunctionKind::Abbreviation { trigger, expansion } => {
                                (trigger.clone(), expansion.clone())
                            }
                            _ => (String::new(), String::new()),
                        };
                        let display = format!("{} → {}", trigger, expansion);

                        h_flex()
                            .py_1()
                            .px_2()
                            .rounded_sm()
                            .justify_between()
                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .flex_1()
                                    .overflow_x_hidden()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(display),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_1()
                                            .rounded_sm()
                                            .bg(cx.theme().colors().element_background)
                                            .text_color(cx.theme().colors().text_muted)
                                            .child(func.protocol.label()),
                                    ),
                            )
                            .child(
                                IconButton::new(
                                    SharedString::from(format!("delete-abbr-{}", func_id)),
                                    IconName::Trash,
                                )
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(t("common.delete")))
                                .on_click(move |_, _window, cx| {
                                    Self::delete_function(func_id, cx);
                                }),
                            )
                    })),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().colors().text_muted)
                    .child(t("function.available_scripts")),
            )
            .child(
                v_flex()
                    .gap_1()
                    .when(all_scripts.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text_muted)
                                .child(t("function.empty_hint").replace("{}", &paths::config_dir_display())),
                        )
                    })
                    .children(all_scripts.iter().map(|script| {
                        let existing_func = functions.iter().find(|f| f.script_path == script.path);
                        let is_enabled = existing_func.map(|f| f.enabled).unwrap_or(false);
                        let func_id = existing_func.map(|f| f.id);
                        let protocol_label = existing_func.map(|f| f.protocol.label());
                        let script_path = script.path.clone();
                        let script_name = script.name.clone();

                        h_flex()
                            .py_1()
                            .px_2()
                            .rounded_sm()
                            .justify_between()
                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .flex_1()
                                    .overflow_x_hidden()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(script_name.clone()),
                                    )
                                    .when_some(protocol_label, |this, label| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .px_1()
                                                .rounded_sm()
                                                .bg(cx.theme().colors().element_background)
                                                .text_color(cx.theme().colors().text_muted)
                                                .child(label),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .when_some(func_id, |this, id| {
                                        this.child(
                                            IconButton::new(
                                                SharedString::from(format!("delete-func-{}", id)),
                                                IconName::Trash,
                                            )
                                            .icon_size(IconSize::Small)
                                            .tooltip(Tooltip::text(t("common.delete")))
                                            .on_click(move |_, _window, cx| {
                                                Self::delete_function(id, cx);
                                            }),
                                        )
                                    })
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
                                    ),
                            )
                    })),
            )
    }
}

fn scripts_dir() -> PathBuf {
    paths::config_dir().join("scripts")
}

fn scan_available_scripts() -> Vec<PathBuf> {
    let dir = scripts_dir();
    let mut scripts = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "py") {
                if path.file_name().is_some_and(|name| name != "bspterm.py") {
                    scripts.push(path);
                }
            }
        }
    }
    scripts.sort();
    scripts
}

fn protocol_button(
    id: &str,
    protocol: TerminalProtocol,
    current: &TerminalProtocol,
    cx: &mut Context<AddFunctionModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

/// Mode for the add function modal.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AddFunctionMode {
    #[default]
    SelectType,
    NewScript,
    SelectExisting,
    NewAbbreviation,
}

/// Modal dialog for adding a new function.
pub struct AddFunctionModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    mode: AddFunctionMode,
    name_editor: Entity<Editor>,
    trigger_editor: Entity<Editor>,
    expansion_editor: Entity<Editor>,
    protocol: TerminalProtocol,
    selected_script: Option<PathBuf>,
    available_scripts: Vec<PathBuf>,
}

impl ModalView for AddFunctionModal {}

impl EventEmitter<DismissEvent> for AddFunctionModal {}

impl Focusable for AddFunctionModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AddFunctionModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        default_protocol: TerminalProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("function.script_name"), window, cx);
            editor
        });

        let trigger_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("function.abbr_trigger_placeholder"), window, cx);
            editor
        });

        let expansion_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("function.abbr_expansion_placeholder"), window, cx);
            editor
        });

        let available_scripts = scan_available_scripts();

        Self {
            focus_handle,
            workspace,
            mode: AddFunctionMode::SelectType,
            name_editor,
            trigger_editor,
            expansion_editor,
            protocol: default_protocol,
            selected_script: None,
            available_scripts,
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn set_protocol(&mut self, protocol: TerminalProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn set_mode(&mut self, mode: AddFunctionMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.selected_script = None;

        self.name_editor.update(cx, |editor, cx| {
            editor.set_text(String::new(), window, cx);
        });
        self.trigger_editor.update(cx, |editor, cx| {
            editor.set_text(String::new(), window, cx);
        });
        self.expansion_editor.update(cx, |editor, cx| {
            editor.set_text(String::new(), window, cx);
        });

        cx.notify();
    }

    fn select_script(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(file_name) = path.file_stem() {
            let label = file_name.to_string_lossy().to_string();
            self.name_editor.update(cx, |editor, cx| {
                editor.set_text(label, window, cx);
            });
        }
        self.selected_script = Some(path);
        cx.notify();
    }

    fn create_new_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.name_editor.read(cx).text(cx).trim().to_string();

        if label.is_empty() {
            return;
        }

        let scripts_directory = scripts_dir();
        if let Err(error) = fs::create_dir_all(&scripts_directory) {
            log::error!("Failed to create scripts directory: {}", error);
            return;
        }

        let script_path = scripts_directory.join(format!("{}.py", label));

        let template = format!(
            r#"#!/usr/bin/env python3
"""
{label} - 终端自动化脚本

# 如需传参，取消下方注释，运行时会弹出参数填写窗口
# 参数在脚本中通过 params.参数名 访问
#
# @params
# - input1: string
#   description: 参数1
#   required: true
#   default: ""
#
# - input2: number
#   description: 参数2
#   default: 0
#
# - input3: boolean
#   description: 参数3
#   default: false
# @end_params
"""
from bspterm import current_terminal
from bspterm import params

term = current_terminal()
# 在此编写你的自动化逻辑
# term.send("命令\n")       # 发送命令
# term.wait_for("模式")     # 等待输出匹配
# output = term.run("命令") # 执行命令并返回输出

# --- 传参示例（全部取消注释即可运行） ---
# input1 = params.input1
# input2 = params.get("input2", 0)
# input3 = params.get("input3", False)
# term.send(f"{{input1}}\n")
"#
        );

        if let Err(error) = fs::write(&script_path, &template) {
            log::error!("Failed to write script file: {}", error);
            return;
        }

        let protocol = self.protocol.clone();
        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let function =
                FunctionConfig::with_protocol(label, script_path.clone(), protocol);
            store.update(cx, |store, cx| {
                store.add_function(function, cx);
            });
        }

        self.dismiss(window, cx);

        let workspace = self.workspace.clone();
        let script_path = script_path.clone();
        cx.defer_in(window, move |_, window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    workspace
                        .open_abs_path(script_path, OpenOptions::default(), window, cx)
                        .detach_and_log_err(cx);
                })
                .ok();
        });
    }

    fn add_existing_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(script_path) = self.selected_script.clone() else {
            return;
        };

        let label = self.name_editor.read(cx).text(cx).trim().to_string();
        if label.is_empty() {
            return;
        }

        let protocol = self.protocol.clone();
        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let function = FunctionConfig::with_protocol(label, script_path, protocol);
            store.update(cx, |store, cx| {
                store.add_function(function, cx);
            });
        }

        self.dismiss(window, cx);
    }

    fn create_abbreviation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.trigger_editor.read(cx).text(cx).trim().to_string();
        let expansion = self.expansion_editor.read(cx).text(cx).trim().to_string();

        if trigger.is_empty() || expansion.is_empty() {
            return;
        }

        let protocol = self.protocol.clone();
        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let function = FunctionConfig::new_abbreviation(trigger, expansion, protocol);
            store.update(cx, |store, cx| {
                store.add_function(function, cx);
            });
        }

        self.dismiss(window, cx);
    }

    fn render_new_abbreviation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.abbr_trigger_word")),
                    )
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.trigger_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.abbr_expansion_text")),
                    )
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.expansion_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.applicable_protocol")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(protocol_button(
                                "protocol-all-abbr",
                                TerminalProtocol::All,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-ssh-abbr",
                                TerminalProtocol::Ssh,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-telnet-abbr",
                                TerminalProtocol::Telnet,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-huawei-abbr",
                                TerminalProtocol::HuaweiVrp,
                                &self.protocol,
                                cx,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        ui::Button::new("cancel-btn", t("common.cancel"))
                            .style(ui::ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_mode(AddFunctionMode::SelectType, window, cx);
                            })),
                    )
                    .child(
                        ui::Button::new("create-btn", t("common.confirm"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_abbreviation(window, cx);
                            })),
                    ),
            )
    }

    fn render_select_type(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                ListItem::new("new-script-option")
                    .spacing(ListItemSpacing::Sparse)
                    .start_slot(
                        ui::Icon::new(IconName::FileCode)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                    .child(t("function.new_python_script"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(AddFunctionMode::NewScript, window, cx);
                    })),
            )
            .child(
                ListItem::new("existing-script-option")
                    .spacing(ListItemSpacing::Sparse)
                    .start_slot(
                        ui::Icon::new(IconName::Folder)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                    .child(t("function.select_existing_script"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(AddFunctionMode::SelectExisting, window, cx);
                    })),
            )
            .child(
                ListItem::new("new-abbreviation-option")
                    .spacing(ListItemSpacing::Sparse)
                    .start_slot(
                        ui::Icon::new(IconName::Replace)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                    .child(t("function.new_abbreviation"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(AddFunctionMode::NewAbbreviation, window, cx);
                    })),
            )
    }

    fn render_new_script(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.script_name")),
                    )
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.name_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.applicable_protocol")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(protocol_button(
                                "protocol-all",
                                TerminalProtocol::All,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-ssh",
                                TerminalProtocol::Ssh,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-telnet",
                                TerminalProtocol::Telnet,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-huawei",
                                TerminalProtocol::HuaweiVrp,
                                &self.protocol,
                                cx,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        ui::Button::new("cancel-btn", t("common.cancel"))
                            .style(ui::ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_mode(AddFunctionMode::SelectType, window, cx);
                            })),
                    )
                    .child(
                        ui::Button::new("create-btn", t("function.create_and_edit"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_new_script(window, cx);
                            })),
                    ),
            )
    }

    fn render_select_existing(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let scripts = self.available_scripts.clone();
        let selected = self.selected_script.clone();

        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .id("script-list")
                    .gap_1()
                    .max_h(px(150.0))
                    .overflow_y_scroll()
                    .when(scripts.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text_muted)
                                .child(t("function.no_scripts_available")),
                        )
                    })
                    .children(scripts.iter().map(|path| {
                        let path_clone = path.clone();
                        let is_selected = selected.as_ref() == Some(path);
                        let file_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        ListItem::new(SharedString::from(format!("script-{}", file_name)))
                            .spacing(ListItemSpacing::Sparse)
                            .start_slot(
                                ui::Icon::new(IconName::FileCode)
                                    .size(IconSize::Small)
                                    .color(if is_selected {
                                        Color::Accent
                                    } else {
                                        Color::Muted
                                    }),
                            )
                            .child(file_name)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_script(path_clone.clone(), window, cx);
                            }))
                    })),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.script_name")),
                    )
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.name_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("function.applicable_protocol")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(protocol_button(
                                "protocol-all-ex",
                                TerminalProtocol::All,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-ssh-ex",
                                TerminalProtocol::Ssh,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-telnet-ex",
                                TerminalProtocol::Telnet,
                                &self.protocol,
                                cx,
                            ))
                            .child(protocol_button(
                                "protocol-huawei-ex",
                                TerminalProtocol::HuaweiVrp,
                                &self.protocol,
                                cx,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        ui::Button::new("cancel-btn", t("common.cancel"))
                            .style(ui::ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_mode(AddFunctionMode::SelectType, window, cx);
                            })),
                    )
                    .child(
                        ui::Button::new("add-btn", t("common.add"))
                            .style(ui::ButtonStyle::Filled)
                            .disabled(self.selected_script.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_existing_script(window, cx);
                            })),
                    ),
            )
    }
}

impl Render for AddFunctionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.mode {
            AddFunctionMode::SelectType => t("function.add_title"),
            AddFunctionMode::NewScript => t("function.new_function_script"),
            AddFunctionMode::SelectExisting => t("function.select_script"),
            AddFunctionMode::NewAbbreviation => t("function.new_abbreviation"),
        };

        v_flex()
            .id("add-function-modal")
            .elevation_3(cx)
            .p_3()
            .gap_3()
            .w(px(400.0))
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
                            .child(title),
                    )
                    .child(
                        IconButton::new("close-modal", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(window, cx);
                            })),
                    ),
            )
            .child(match self.mode {
                AddFunctionMode::SelectType => self.render_select_type(cx).into_any_element(),
                AddFunctionMode::NewScript => self.render_new_script(cx).into_any_element(),
                AddFunctionMode::SelectExisting => {
                    self.render_select_existing(cx).into_any_element()
                }
                AddFunctionMode::NewAbbreviation => {
                    self.render_new_abbreviation(cx).into_any_element()
                }
            })
    }
}

fn rename_protocol_button(
    id: &str,
    protocol: TerminalProtocol,
    current: &TerminalProtocol,
    cx: &mut Context<RenameFunctionModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol_clone.clone(), cx);
        }))
}

/// Modal dialog for renaming a function (name + protocol only).
pub struct RenameFunctionModal {
    focus_handle: FocusHandle,
    func_id: Uuid,
    name_editor: Entity<Editor>,
    protocol: TerminalProtocol,
}

impl ModalView for RenameFunctionModal {}

impl EventEmitter<DismissEvent> for RenameFunctionModal {}

impl Focusable for RenameFunctionModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl RenameFunctionModal {
    pub fn new(func_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = FunctionStoreEntity::global(cx);
        let func = store.read(cx).find_function(func_id);

        let (name_text, protocol) = func
            .map(|f| (f.name.clone(), f.protocol.clone()))
            .unwrap_or_default();

        let name_editor = cx.new(|cx| {
            let mut ed = Editor::single_line(window, cx);
            ed.set_text(name_text, window, cx);
            ed
        });

        Self {
            focus_handle,
            func_id,
            name_editor,
            protocol,
        }
    }

    fn set_protocol(&mut self, protocol: TerminalProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);

        if name.is_empty() {
            cx.emit(DismissEvent);
            return;
        }

        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let func_id = self.func_id;
            let protocol = self.protocol.clone();
            store.update(cx, |store, cx| {
                store.update_function(
                    func_id,
                    move |func| {
                        func.name = name;
                        func.protocol = protocol;
                    },
                    cx,
                );
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for RenameFunctionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("RenameFunctionModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(400.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("function.edit_name_title")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.name_label")).size(LabelSize::Small))
                            .child(self.name_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(rename_protocol_button(
                                        "rename-all",
                                        TerminalProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(rename_protocol_button(
                                        "rename-ssh",
                                        TerminalProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(rename_protocol_button(
                                        "rename-telnet",
                                        TerminalProtocol::Telnet,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(rename_protocol_button(
                                        "rename-huawei",
                                        TerminalProtocol::HuaweiVrp,
                                        &current_protocol,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", t("common.cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(&menu::Cancel, window, cx)
                            })),
                    )
                    .child(
                        ui::Button::new("confirm", t("common.confirm"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(&menu::Confirm, window, cx)
                            })),
                    ),
            )
    }
}

fn edit_abbr_protocol_button(
    id: &str,
    protocol: TerminalProtocol,
    current: &TerminalProtocol,
    cx: &mut Context<EditAbbreviationModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = protocol.label();
    let protocol_clone = protocol.clone();

    ui::Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ui::ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ui::ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.protocol = protocol_clone.clone();
            cx.notify();
        }))
}

/// Modal dialog for editing an abbreviation (trigger, expansion, protocol).
pub struct EditAbbreviationModal {
    focus_handle: FocusHandle,
    func_id: Uuid,
    trigger_editor: Entity<Editor>,
    expansion_editor: Entity<Editor>,
    protocol: TerminalProtocol,
}

impl ModalView for EditAbbreviationModal {}

impl EventEmitter<DismissEvent> for EditAbbreviationModal {}

impl Focusable for EditAbbreviationModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditAbbreviationModal {
    pub fn new(func_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = FunctionStoreEntity::global(cx);
        let func = store.read(cx).find_function(func_id);

        let (trigger_text, expansion_text, protocol) = func
            .map(|f| {
                let (trigger, expansion) = match &f.kind {
                    FunctionKind::Abbreviation { trigger, expansion } => {
                        (trigger.clone(), expansion.clone())
                    }
                    _ => (String::new(), String::new()),
                };
                (trigger, expansion, f.protocol.clone())
            })
            .unwrap_or_default();

        let trigger_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(trigger_text, window, cx);
            editor
        });

        let expansion_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(expansion_text, window, cx);
            editor
        });

        Self {
            focus_handle,
            func_id,
            trigger_editor,
            expansion_editor,
            protocol,
        }
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let trigger = self.trigger_editor.read(cx).text(cx).trim().to_string();
        let expansion = self.expansion_editor.read(cx).text(cx).trim().to_string();

        if trigger.is_empty() || expansion.is_empty() {
            cx.emit(DismissEvent);
            return;
        }

        if let Some(store) = FunctionStoreEntity::try_global(cx) {
            let func_id = self.func_id;
            let protocol = self.protocol.clone();
            store.update(cx, |store, cx| {
                store.update_function(
                    func_id,
                    move |func| {
                        func.name = trigger.clone();
                        func.protocol = protocol;
                        func.kind = FunctionKind::Abbreviation { trigger, expansion };
                    },
                    cx,
                );
            });
        }

        cx.emit(DismissEvent);
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for EditAbbreviationModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_protocol = self.protocol.clone();

        v_flex()
            .key_context("EditAbbreviationModal")
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(400.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(t("function.edit_abbreviation")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.abbr_trigger_word")).size(LabelSize::Small))
                            .child(self.trigger_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.abbr_expansion_text")).size(LabelSize::Small))
                            .child(self.expansion_editor.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new(t("function.applicable_protocol")).size(LabelSize::Small))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(edit_abbr_protocol_button(
                                        "edit-abbr-all",
                                        TerminalProtocol::All,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_abbr_protocol_button(
                                        "edit-abbr-ssh",
                                        TerminalProtocol::Ssh,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_abbr_protocol_button(
                                        "edit-abbr-telnet",
                                        TerminalProtocol::Telnet,
                                        &current_protocol,
                                        cx,
                                    ))
                                    .child(edit_abbr_protocol_button(
                                        "edit-abbr-huawei",
                                        TerminalProtocol::HuaweiVrp,
                                        &current_protocol,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::Button::new("cancel", t("common.cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(&menu::Cancel, window, cx)
                            })),
                    )
                    .child(
                        ui::Button::new("confirm", t("common.confirm"))
                            .style(ui::ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(&menu::Confirm, window, cx)
                            })),
                    ),
            )
    }
}
