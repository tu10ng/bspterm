use std::fs;
use std::path::PathBuf;

use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyContext, ParentElement, Render, Styled, Subscription, WeakEntity, Window,
};
use i18n::t;
use terminal::{
    TerminalProtocol, get_action_label, Clear, ClearScrollback, Copy, Paste, ScrollLineDown,
    ScrollLineUp, ScrollPageDown, ScrollPageUp, ScrollToBottom, ScrollToTop,
    ShortcutBarStoreEntity, ShortcutBarStoreEvent, ToggleViMode, ALL_SYSTEM_ACTIONS,
};

use super::{DisconnectTerminal, ReconnectTerminal};
use ui::{
    Button, ButtonCommon, ButtonStyle, Color, IconButton, IconName, IconSize,
    KeystrokeRecorder, ListItem, ListItemSpacing, Switch, TintColor, ToggleState, Tooltip, prelude::*,
};
use uuid::Uuid;
use workspace::{ModalView, OpenOptions, Workspace};

use editor::actions::SelectAll;

/// A display item for a system shortcut (keybinding + action).
#[derive(Clone)]
pub struct SystemShortcutItem {
    pub keybinding: String,
    pub action_type: String,
    pub label: String,
}

/// Get all keybindings for a given action type from the keymap.
pub fn get_keybindings_for_action(action_type: &str, window: &Window) -> Vec<String> {
    let Some(terminal_context) = KeyContext::parse("Terminal").ok() else {
        return Vec::new();
    };

    let bindings: Vec<_> = match action_type {
        "terminal::Copy" => window.bindings_for_action_in_context(&Copy, terminal_context),
        "terminal::Paste" => window.bindings_for_action_in_context(&Paste, terminal_context),
        "terminal::Clear" => window.bindings_for_action_in_context(&Clear, terminal_context),
        "terminal::ClearScrollback" => {
            window.bindings_for_action_in_context(&ClearScrollback, terminal_context)
        }
        "terminal::ScrollPageUp" => {
            window.bindings_for_action_in_context(&ScrollPageUp, terminal_context)
        }
        "terminal::ScrollPageDown" => {
            window.bindings_for_action_in_context(&ScrollPageDown, terminal_context)
        }
        "terminal::ScrollToTop" => {
            window.bindings_for_action_in_context(&ScrollToTop, terminal_context)
        }
        "terminal::ScrollToBottom" => {
            window.bindings_for_action_in_context(&ScrollToBottom, terminal_context)
        }
        "terminal::ScrollLineUp" => {
            window.bindings_for_action_in_context(&ScrollLineUp, terminal_context)
        }
        "terminal::ScrollLineDown" => {
            window.bindings_for_action_in_context(&ScrollLineDown, terminal_context)
        }
        "terminal::ToggleViMode" => {
            window.bindings_for_action_in_context(&ToggleViMode, terminal_context)
        }
        "terminal::ReconnectTerminal" => {
            window.bindings_for_action_in_context(&ReconnectTerminal, terminal_context)
        }
        "terminal::DisconnectTerminal" => {
            window.bindings_for_action_in_context(&DisconnectTerminal, terminal_context)
        }
        "editor::SelectAll" => {
            window.bindings_for_action_in_context(&SelectAll, terminal_context)
        }
        _ => return Vec::new(),
    };

    bindings
        .iter()
        .map(|binding| {
            binding
                .keystrokes()
                .iter()
                .map(|k| k.unparse())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Get all system shortcuts with their keybindings from the keymap.
pub fn get_all_system_shortcuts(window: &Window) -> Vec<SystemShortcutItem> {
    let mut items = Vec::new();

    for action_type in ALL_SYSTEM_ACTIONS {
        let keybindings = get_keybindings_for_action(action_type, window);
        let label = get_action_label(action_type).to_string();

        for keybinding in keybindings {
            items.push(SystemShortcutItem {
                keybinding,
                action_type: action_type.to_string(),
                label: label.clone(),
            });
        }
    }

    items
}

fn scripts_dir() -> PathBuf {
    paths::config_dir().join("scripts")
}

fn scan_scripts() -> Vec<PathBuf> {
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

/// Configuration modal for the shortcut bar.
pub struct ShortcutBarConfigModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    _subscription: Subscription,
}

impl ModalView for ShortcutBarConfigModal {}

impl EventEmitter<DismissEvent> for ShortcutBarConfigModal {}

impl ShortcutBarConfigModal {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let subscription = cx.subscribe(
            &ShortcutBarStoreEntity::global(cx),
            |_this, _, _event: &ShortcutBarStoreEvent, cx| {
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

    fn open_add_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(window, cx);
        let workspace = self.workspace.clone();
        cx.defer_in(window, move |_, window, cx| {
            workspace
                .update(cx, |ws, cx| {
                    let ws_handle = ws.weak_handle();
                    ws.toggle_modal(window, cx, |window, cx| {
                        AddShortcutModal::new(ws_handle, window, cx)
                    });
                })
                .ok();
        });
    }

    fn set_system_shortcut_visible(keybinding: &str, action: &str, visible: bool, cx: &mut App) {
        let Some(store) = ShortcutBarStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.set_system_shortcut_visible(keybinding, action, visible, cx);
        });
    }

    fn set_script_shortcut_hidden(id: Uuid, hidden: bool, cx: &mut App) {
        let Some(store) = ShortcutBarStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.set_script_shortcut_hidden(id, hidden, cx);
        });
    }

    fn delete_script_shortcut(id: Uuid, cx: &mut App) {
        let Some(store) = ShortcutBarStoreEntity::try_global(cx) else {
            return;
        };

        store.update(cx, |store, cx| {
            store.remove_script_shortcut(id, cx);
        });
    }

    fn open_edit_modal_static(workspace: &WeakEntity<Workspace>, id: Uuid, window: &mut Window, cx: &mut App) {
        workspace
            .update(cx, |ws, cx| {
                ws.toggle_modal(window, cx, |window, cx| EditShortcutModal::new(id, window, cx));
            })
            .ok();
    }
}

impl Focusable for ShortcutBarConfigModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ShortcutBarConfigModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = ShortcutBarStoreEntity::global(cx);
        let store_ref = store.read(cx);
        let workspace = self.workspace.clone();

        let system_shortcuts = get_all_system_shortcuts(window);
        let script_shortcuts: Vec<_> = store_ref.script_shortcuts().to_vec();

        v_flex()
            .id("shortcut-bar-config-modal")
            .elevation_3(cx)
            .p_3()
            .gap_2()
            .w(px(420.0))
            .max_h(px(500.0))
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
                            .child(t("shortcut.config_title")),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                IconButton::new("add-shortcut-btn", IconName::Plus)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text(t("shortcut.add_script_shortcut")))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_add_modal(window, cx);
                                    })),
                            )
                            .child(
                                IconButton::new("close-modal", IconName::Close)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .id("shortcut-list-container")
                    .gap_2()
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(cx.theme().colors().text_muted)
                            .child(t("shortcut.system_shortcuts")),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .children(system_shortcuts.iter().enumerate().map(|(idx, item)| {
                                let is_visible = store_ref.is_system_shortcut_visible(&item.keybinding, &item.action_type);
                                let keybinding = item.keybinding.clone();
                                let action = item.action_type.clone();

                                h_flex()
                                    .id(SharedString::from(format!("system-shortcut-{}", idx)))
                                    .w_full()
                                    .py_1()
                                    .px_2()
                                    .rounded_sm()
                                    .justify_between()
                                    .items_center()
                                    .hover(|s| s.bg(cx.theme().colors().element_hover))
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .items_center()
                                            .flex_1()
                                            .overflow_x_hidden()
                                            .child(
                                                div()
                                                    .min_w(px(100.0))
                                                    .max_w(px(120.0))
                                                    .text_sm()
                                                    .text_color(cx.theme().colors().text_muted)
                                                    .overflow_x_hidden()
                                                    .text_ellipsis()
                                                    .child(item.keybinding.clone()),
                                            )
                                            .child(div().text_sm().child("→"))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .overflow_x_hidden()
                                                    .text_ellipsis()
                                                    .child(item.label.clone()),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .items_center()
                                            .child(
                                                Switch::new(
                                                    SharedString::from(format!("system-show-{}", idx)),
                                                    if is_visible {
                                                        ToggleState::Selected
                                                    } else {
                                                        ToggleState::Unselected
                                                    },
                                                )
                                                .on_click(move |state, _window, cx| {
                                                    let visible = *state == ToggleState::Selected;
                                                    Self::set_system_shortcut_visible(&keybinding, &action, visible, cx);
                                                }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().colors().text_muted)
                                                    .child(t("shortcut.show")),
                                            ),
                                    )
                            })),
                    )
                    .when(!script_shortcuts.is_empty(), |this| {
                        this.child(
                            div()
                                .pt_2()
                                .child(ui::Divider::horizontal_dashed()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(cx.theme().colors().text_muted)
                                .child(t("shortcut.script_shortcuts")),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .children(script_shortcuts.iter().enumerate().map(|(idx, shortcut)| {
                                    let is_hidden = shortcut.hidden;
                                    let shortcut_id = shortcut.id;
                                    let workspace_for_edit = workspace.clone();

                                    h_flex()
                                        .id(SharedString::from(format!("script-shortcut-{}", idx)))
                                        .w_full()
                                        .py_1()
                                        .px_2()
                                        .rounded_sm()
                                        .justify_between()
                                        .items_center()
                                        .hover(|s| s.bg(cx.theme().colors().element_hover))
                                        .child(
                                            h_flex()
                                                .gap_1()
                                                .items_center()
                                                .flex_1()
                                                .overflow_x_hidden()
                                                .child(
                                                    div()
                                                        .min_w(px(100.0))
                                                        .max_w(px(120.0))
                                                        .text_sm()
                                                        .text_color(cx.theme().colors().text_muted)
                                                        .overflow_x_hidden()
                                                        .text_ellipsis()
                                                        .child(shortcut.keybinding.clone()),
                                                )
                                                .child(div().text_sm().child("→"))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(gpui::FontWeight::MEDIUM)
                                                        .overflow_x_hidden()
                                                        .text_ellipsis()
                                                        .child(shortcut.label.clone()),
                                                ),
                                        )
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .items_center()
                                                .child(
                                                    h_flex()
                                                        .gap_1()
                                                        .items_center()
                                                        .child(
                                                            Switch::new(
                                                                SharedString::from(format!("script-show-{}", idx)),
                                                                if !is_hidden {
                                                                    ToggleState::Selected
                                                                } else {
                                                                    ToggleState::Unselected
                                                                },
                                                            )
                                                            .on_click(move |state, _window, cx| {
                                                                let hidden = *state != ToggleState::Selected;
                                                                Self::set_script_shortcut_hidden(shortcut_id, hidden, cx);
                                                            }),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(cx.theme().colors().text_muted)
                                                                .child(t("shortcut.show")),
                                                        ),
                                                )
                                                .child(
                                                    IconButton::new(
                                                        SharedString::from(format!("edit-script-{}", idx)),
                                                        IconName::Pencil,
                                                    )
                                                    .icon_size(IconSize::Small)
                                                    .icon_color(Color::Muted)
                                                    .tooltip(Tooltip::text(t("shortcut.edit")))
                                                    .on_click(move |_, window, cx| {
                                                        Self::open_edit_modal_static(&workspace_for_edit, shortcut_id, window, cx);
                                                    }),
                                                )
                                                .child(
                                                    IconButton::new(
                                                        SharedString::from(format!("delete-script-{}", idx)),
                                                        IconName::Trash,
                                                    )
                                                    .icon_size(IconSize::Small)
                                                    .icon_color(Color::Muted)
                                                    .tooltip(Tooltip::text(t("shortcut.delete")))
                                                    .on_click(move |_, _window, cx| {
                                                        Self::delete_script_shortcut(shortcut_id, cx);
                                                    }),
                                                ),
                                        )
                                })),
                        )
                    }),
            )
    }
}

/// Mode for the add shortcut modal.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AddShortcutMode {
    #[default]
    SelectType,
    NewScript,
    SelectExisting,
}

fn shortcut_protocol_button(
    id: &str,
    protocol: Option<TerminalProtocol>,
    current: &Option<TerminalProtocol>,
    cx: &mut Context<AddShortcutModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = match &protocol {
        None => t("shortcut.scope_all"),
        Some(TerminalProtocol::All) => t("shortcut.scope_all"),
        Some(TerminalProtocol::Ssh) => "SSH".into(),
        Some(TerminalProtocol::Telnet) => "Telnet".into(),
        Some(TerminalProtocol::HuaweiVrp) => "华为设备".into(),
    };

    Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol.clone(), cx);
        }))
}

/// Modal for adding a new script shortcut.
pub struct AddShortcutModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    mode: AddShortcutMode,
    label_editor: Entity<Editor>,
    keybinding_recorder: Entity<KeystrokeRecorder>,
    selected_script: Option<PathBuf>,
    available_scripts: Vec<PathBuf>,
    selected_protocol: Option<TerminalProtocol>,
    _subscription: Subscription,
}

impl ModalView for AddShortcutModal {}

impl EventEmitter<DismissEvent> for AddShortcutModal {}

impl AddShortcutModal {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let label_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("shortcut.script_name"), window, cx);
            editor
        });

        let keybinding_recorder = cx.new(|cx| KeystrokeRecorder::new(None, window, cx));

        let available_scripts = scan_scripts();

        let subscription = cx.subscribe(
            &ShortcutBarStoreEntity::global(cx),
            |_this, _, _event: &ShortcutBarStoreEvent, cx| {
                cx.notify();
            },
        );

        Self {
            focus_handle,
            workspace,
            mode: AddShortcutMode::SelectType,
            label_editor,
            keybinding_recorder,
            selected_script: None,
            available_scripts,
            selected_protocol: None,
            _subscription: subscription,
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn set_protocol(&mut self, protocol: Option<TerminalProtocol>, cx: &mut Context<Self>) {
        self.selected_protocol = protocol;
        cx.notify();
    }

    fn set_mode(&mut self, mode: AddShortcutMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.selected_script = None;

        self.label_editor.update(cx, |editor, cx| {
            editor.set_text(String::new(), window, cx);
        });
        self.keybinding_recorder.update(cx, |recorder, _cx| {
            recorder.set_value(None);
        });

        cx.notify();
    }

    fn select_script(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(file_name) = path.file_stem() {
            let label = file_name.to_string_lossy().to_string();
            self.label_editor.update(cx, |editor, cx| {
                editor.set_text(label, window, cx);
            });
        }
        self.selected_script = Some(path);
        cx.notify();
    }

    fn create_new_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.label_editor.read(cx).text(cx).trim().to_string();
        let keybinding = self
            .keybinding_recorder
            .read(cx)
            .current_value()
            .unwrap_or_default();

        if label.is_empty() {
            return;
        }

        let scripts_dir = scripts_dir();
        if let Err(e) = fs::create_dir_all(&scripts_dir) {
            log::error!("Failed to create scripts directory: {}", e);
            return;
        }

        let script_path = scripts_dir.join(format!("{}.py", label));

        let template = format!(
            r#"#!/usr/bin/env python3
"""
{} - 终端自动化脚本

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
"#,
            label
        );

        if let Err(e) = fs::write(&script_path, &template) {
            log::error!("Failed to write script file: {}", e);
            return;
        }

        let protocol = self.selected_protocol.clone();
        if let Some(store) = ShortcutBarStoreEntity::try_global(cx) {
            store.update(cx, |store, cx| {
                store.add_script_shortcut(label, keybinding, script_path.clone(), protocol, cx);
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

        let label = self.label_editor.read(cx).text(cx).trim().to_string();
        let keybinding = self
            .keybinding_recorder
            .read(cx)
            .current_value()
            .unwrap_or_default();

        if label.is_empty() {
            return;
        }

        let protocol = self.selected_protocol.clone();
        if let Some(store) = ShortcutBarStoreEntity::try_global(cx) {
            store.update(cx, |store, cx| {
                store.add_script_shortcut(label, keybinding, script_path, protocol, cx);
            });
        }

        self.dismiss(window, cx);
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
                    .child(t("shortcut.new_python_script"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(AddShortcutMode::NewScript, window, cx);
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
                    .child(t("shortcut.select_existing_script"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_mode(AddShortcutMode::SelectExisting, window, cx);
                    })),
            )
    }

    fn render_new_script(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.script_name")))
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.label_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.shortcut_key")))
                    .child(self.keybinding_recorder.clone()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.scope")))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(shortcut_protocol_button(
                                "protocol-all",
                                None,
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-ssh",
                                Some(TerminalProtocol::Ssh),
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-telnet",
                                Some(TerminalProtocol::Telnet),
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-huawei",
                                Some(TerminalProtocol::HuaweiVrp),
                                &self.selected_protocol,
                                cx,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-btn", t("common.cancel"))
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_mode(AddShortcutMode::SelectType, window, cx);
                            })),
                    )
                    .child(
                        Button::new("create-btn", t("shortcut.create_and_edit"))
                            .style(ButtonStyle::Filled)
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
                                .child(t("shortcut.no_scripts_available")),
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
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.script_name")))
                    .child(
                        div()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .child(self.label_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.shortcut_key")))
                    .child(self.keybinding_recorder.clone()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(t("shortcut.scope")))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(shortcut_protocol_button(
                                "protocol-all-ex",
                                None,
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-ssh-ex",
                                Some(TerminalProtocol::Ssh),
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-telnet-ex",
                                Some(TerminalProtocol::Telnet),
                                &self.selected_protocol,
                                cx,
                            ))
                            .child(shortcut_protocol_button(
                                "protocol-huawei-ex",
                                Some(TerminalProtocol::HuaweiVrp),
                                &self.selected_protocol,
                                cx,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-btn", t("common.cancel"))
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_mode(AddShortcutMode::SelectType, window, cx);
                            })),
                    )
                    .child(
                        Button::new("add-btn", t("common.add"))
                            .style(ButtonStyle::Filled)
                            .disabled(self.selected_script.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_existing_script(window, cx);
                            })),
                    ),
            )
    }
}

impl Focusable for AddShortcutModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AddShortcutModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.mode {
            AddShortcutMode::SelectType => t("shortcut.add_script_shortcut"),
            AddShortcutMode::NewScript => t("shortcut.new_shortcut_script"),
            AddShortcutMode::SelectExisting => t("shortcut.select_script"),
        };

        v_flex()
            .id("add-shortcut-modal")
            .elevation_3(cx)
            .p_3()
            .gap_3()
            .w(px(360.0))
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
                AddShortcutMode::SelectType => self.render_select_type(cx).into_any_element(),
                AddShortcutMode::NewScript => self.render_new_script(cx).into_any_element(),
                AddShortcutMode::SelectExisting => {
                    self.render_select_existing(cx).into_any_element()
                }
            })
    }
}

fn edit_shortcut_protocol_button(
    id: &str,
    protocol: Option<TerminalProtocol>,
    current: &Option<TerminalProtocol>,
    cx: &mut Context<EditShortcutModal>,
) -> impl IntoElement {
    let is_selected = &protocol == current;
    let label = match &protocol {
        None => t("shortcut.scope_all"),
        Some(TerminalProtocol::All) => t("shortcut.scope_all"),
        Some(TerminalProtocol::Ssh) => "SSH".into(),
        Some(TerminalProtocol::Telnet) => "Telnet".into(),
        Some(TerminalProtocol::HuaweiVrp) => "华为设备".into(),
    };

    Button::new(SharedString::from(id.to_string()), label)
        .style(if is_selected {
            ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ButtonStyle::Subtle
        })
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_protocol(protocol.clone(), cx);
        }))
}

/// Modal for editing an existing script shortcut.
pub struct EditShortcutModal {
    focus_handle: FocusHandle,
    shortcut_id: Uuid,
    label_editor: Entity<Editor>,
    keybinding_recorder: Entity<KeystrokeRecorder>,
    selected_protocol: Option<TerminalProtocol>,
    _subscription: Subscription,
}

impl ModalView for EditShortcutModal {}

impl EventEmitter<DismissEvent> for EditShortcutModal {}

impl EditShortcutModal {
    pub fn new(shortcut_id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let store = ShortcutBarStoreEntity::global(cx);
        let shortcut = store.read(cx).find_script_shortcut(shortcut_id);

        let (label_text, keybinding_text, protocol) = shortcut
            .map(|s| (s.label.clone(), s.keybinding.clone(), s.protocol.clone()))
            .unwrap_or_default();

        let label_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(label_text, window, cx);
            editor
        });

        let keybinding_recorder = cx.new(|cx| {
            KeystrokeRecorder::new(
                if keybinding_text.is_empty() {
                    None
                } else {
                    Some(keybinding_text)
                },
                window,
                cx,
            )
        });

        let subscription = cx.subscribe(
            &ShortcutBarStoreEntity::global(cx),
            |_this, _, _event: &ShortcutBarStoreEvent, cx| {
                cx.notify();
            },
        );

        Self {
            focus_handle,
            shortcut_id,
            label_editor,
            keybinding_recorder,
            selected_protocol: protocol,
            _subscription: subscription,
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn set_protocol(&mut self, protocol: Option<TerminalProtocol>, cx: &mut Context<Self>) {
        self.selected_protocol = protocol;
        cx.notify();
    }

    fn save_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.label_editor.read(cx).text(cx).trim().to_string();
        let keybinding = self
            .keybinding_recorder
            .read(cx)
            .current_value()
            .unwrap_or_default();

        if label.is_empty() {
            return;
        }

        let protocol = self.selected_protocol.clone();
        let shortcut_id = self.shortcut_id;

        if let Some(store) = ShortcutBarStoreEntity::try_global(cx) {
            store.update(cx, |store, cx| {
                store.update_script_shortcut(
                    shortcut_id,
                    move |shortcut| {
                        shortcut.label = label;
                        shortcut.keybinding = keybinding;
                        shortcut.protocol = protocol;
                    },
                    cx,
                );
            });
        }

        self.dismiss(window, cx);
    }
}

impl Focusable for EditShortcutModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditShortcutModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("edit-shortcut-modal")
            .elevation_3(cx)
            .p_3()
            .gap_3()
            .w(px(360.0))
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
                            .child(t("shortcut.edit")),
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
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(t("shortcut.script_name")),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .child(self.label_editor.clone()),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(t("shortcut.shortcut_key")),
                            )
                            .child(self.keybinding_recorder.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(t("shortcut.scope")),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(edit_shortcut_protocol_button(
                                        "edit-protocol-all",
                                        None,
                                        &self.selected_protocol,
                                        cx,
                                    ))
                                    .child(edit_shortcut_protocol_button(
                                        "edit-protocol-ssh",
                                        Some(TerminalProtocol::Ssh),
                                        &self.selected_protocol,
                                        cx,
                                    ))
                                    .child(edit_shortcut_protocol_button(
                                        "edit-protocol-telnet",
                                        Some(TerminalProtocol::Telnet),
                                        &self.selected_protocol,
                                        cx,
                                    ))
                                    .child(edit_shortcut_protocol_button(
                                        "edit-protocol-huawei",
                                        Some(TerminalProtocol::HuaweiVrp),
                                        &self.selected_protocol,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_end()
                            .child(
                                Button::new("cancel-btn", t("common.cancel"))
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("save-btn", t("common.save"))
                                    .style(ButtonStyle::Filled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_changes(window, cx);
                                    })),
                            ),
                    ),
            )
    }
}
