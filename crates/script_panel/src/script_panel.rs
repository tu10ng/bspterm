pub mod script_runner;

use std::path::PathBuf;

use anyhow::Result;
use gpui::{
    Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, WeakEntity, Window, px,
};
use ui::{
    prelude::*, Color, Icon, IconName, IconSize, Label, LabelSize, ListItem, ListItemSpacing,
    h_flex, v_flex,
};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};
use bspterm_actions::script_panel::ToggleFocus;

use script_runner::{ScriptRunner, ScriptStatus};

const SCRIPT_PANEL_KEY: &str = "ScriptPanel";

const BSPTERM_PY: &[u8] = include_bytes!("../../../assets/scripts/bspterm.py");

const DEFAULT_SCRIPTS: &[(&str, &[u8])] = &[
    (
        "ne5000e_mpu_collector.py",
        include_bytes!("../../../assets/scripts/ne5000e_mpu_collector.py"),
    ),
];

fn scripts_dir() -> PathBuf {
    paths::config_dir().join("scripts")
}

fn ensure_default_scripts() {
    let scripts_dir = scripts_dir();

    if let Err(e) = std::fs::create_dir_all(&scripts_dir) {
        log::error!("Failed to create scripts directory: {}", e);
        return;
    }

    let bspterm_path = scripts_dir.join("bspterm.py");
    if let Err(e) = std::fs::write(&bspterm_path, BSPTERM_PY) {
        log::error!("Failed to write bspterm.py: {}", e);
    } else {
        log::info!("Installed bspterm.py to {:?}", bspterm_path);
    }

    for (name, content) in DEFAULT_SCRIPTS {
        let script_path = scripts_dir.join(name);
        if let Err(e) = std::fs::write(&script_path, content) {
            log::error!("Failed to write {}: {}", name, e);
        } else {
            log::info!("Installed {} to {:?}", name, script_path);
        }
    }
}

pub fn init(cx: &mut App) {
    ensure_default_scripts();

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<ScriptPanel>(window, cx);
        });
    })
    .detach();
}

#[derive(Clone)]
struct ScriptEntry {
    name: String,
    path: PathBuf,
}

pub struct ScriptPanel {
    focus_handle: FocusHandle,
    #[allow(dead_code)]
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    scripts: Vec<ScriptEntry>,
    selected_script: Option<usize>,
    script_runner: Option<ScriptRunner>,
    output: String,
    _subscription: Option<gpui::Subscription>,
}

impl ScriptPanel {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let scripts = Self::load_scripts();

        Self {
            focus_handle,
            workspace,
            width: None,
            scripts,
            selected_script: None,
            script_runner: None,
            output: String::new(),
            _subscription: None,
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: gpui::AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        cx.update(|_window, cx| cx.new(|cx| Self::new(workspace, cx)))
    }

    fn load_scripts() -> Vec<ScriptEntry> {
        let scripts_dir = scripts_dir();
        let mut scripts = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                // Skip bspterm.py - it's the library, not a user script
                if path.file_name().is_some_and(|n| n == "bspterm.py") {
                    continue;
                }
                if path.extension().is_some_and(|ext| ext == "py") {
                    if let Some(name) = path.file_stem() {
                        scripts.push(ScriptEntry {
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

    fn refresh_scripts(&mut self, _cx: &mut Context<Self>) {
        self.scripts = Self::load_scripts();
    }

    fn run_script(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.scripts.len() {
            return;
        }

        let script = &self.scripts[index];
        self.output.clear();
        self.output.push_str(&format!("Running {}...\n", script.name));

        let focused_terminal_id = terminal_scripting::TerminalRegistry::focused_id(cx)
            .map(|id| id.to_string());

        let socket_path = terminal_scripting::ScriptingServer::get(cx);

        if socket_path.is_none() {
            self.output.push_str("Error: Scripting server not running\n");
            cx.notify();
            return;
        }

        let runner = ScriptRunner::new(
            script.path.clone(),
            socket_path.unwrap(),
            focused_terminal_id,
        );

        self.script_runner = Some(runner);
        self.selected_script = Some(index);

        let script_runner = self.script_runner.as_mut().unwrap();
        if let Err(e) = script_runner.start() {
            self.output.push_str(&format!("Failed to start: {}\n", e));
            self.script_runner = None;
        }

        cx.notify();
    }

    fn stop_script(&mut self, _cx: &mut Context<Self>) {
        if let Some(runner) = &mut self.script_runner {
            runner.stop();
            self.output.push_str("Script stopped.\n");
        }
        self.script_runner = None;
    }

    fn update_output(&mut self, cx: &mut Context<Self>) {
        if let Some(runner) = &mut self.script_runner {
            if let Some(output) = runner.read_output() {
                self.output.push_str(&output);
                cx.notify();
            }

            match runner.status() {
                ScriptStatus::Finished(code) => {
                    self.output
                        .push_str(&format!("\nScript finished with exit code: {}\n", code));
                    self.script_runner = None;
                    cx.notify();
                }
                ScriptStatus::Failed(err) => {
                    self.output.push_str(&format!("\nScript failed: {}\n", err));
                    self.script_runner = None;
                    cx.notify();
                }
                _ => {}
            }
        }
    }

    fn render_script_list(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let scripts = self.scripts.clone();
        let selected = self.selected_script;
        let is_running = self.script_runner.is_some();

        v_flex().w_full().gap_px().children(
            scripts
                .into_iter()
                .enumerate()
                .map(|(index, script)| {
                    let is_selected = selected == Some(index);

                    ListItem::new(("script", index))
                        .spacing(ListItemSpacing::Dense)
                        .toggle_state(is_selected)
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Icon::new(IconName::FileCode)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(Label::new(script.name).size(LabelSize::Small)),
                        )
                        .on_click(cx.listener(move |this, _, _window, cx| {
                            this.selected_script = Some(index);
                            cx.notify();
                        }))
                        .when(!is_running, |item| {
                            item.on_secondary_mouse_down(cx.listener(
                                move |this, _, window, cx| {
                                    this.run_script(index, window, cx);
                                },
                            ))
                        })
                })
                .collect::<Vec<_>>(),
        )
    }

    fn render_controls(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_running = self.script_runner.is_some();
        let has_selection = self.selected_script.is_some();
        let selected_index = self.selected_script;

        h_flex()
            .w_full()
            .gap_2()
            .p_2()
            .child(
                ui::Button::new("refresh", "Refresh")
                    .style(ui::ButtonStyle::Subtle)
                    .size(ui::ButtonSize::Compact)
                    .icon(IconName::ArrowCircle)
                    .icon_size(IconSize::Small)
                    .icon_position(ui::IconPosition::Start)
                    .disabled(is_running)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.refresh_scripts(cx);
                        cx.notify();
                    })),
            )
            .when(has_selection && !is_running, |this| {
                this.child(
                    ui::Button::new("run", "Run")
                        .style(ui::ButtonStyle::Filled)
                        .size(ui::ButtonSize::Compact)
                        .icon(IconName::PlayFilled)
                        .icon_size(IconSize::Small)
                        .icon_position(ui::IconPosition::Start)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(index) = selected_index {
                                this.run_script(index, window, cx);
                            }
                        })),
                )
            })
            .when(is_running, |this| {
                this.child(
                    ui::Button::new("stop", "Stop")
                        .style(ui::ButtonStyle::Filled)
                        .size(ui::ButtonSize::Compact)
                        .icon(IconName::Stop)
                        .icon_size(IconSize::Small)
                        .icon_position(ui::IconPosition::Start)
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.stop_script(cx);
                            cx.notify();
                        })),
                )
            })
    }

    fn render_output(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_output(cx);

        let output = self.output.clone();

        v_flex()
            .id("script-output")
            .w_full()
            .flex_1()
            .p_2()
            .bg(cx.theme().colors().editor_background)
            .overflow_y_scroll()
            .child(
                Label::new(if output.is_empty() {
                    "Output will appear here...".to_string()
                } else {
                    output
                })
                .size(LabelSize::Small)
                .color(if self.output.is_empty() {
                    Color::Muted
                } else {
                    Color::Default
                }),
            )
    }
}

impl EventEmitter<PanelEvent> for ScriptPanel {}

impl Render for ScriptPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let panel_bg = colors.panel_background;
        let border_color = colors.border;

        v_flex()
            .key_context("ScriptPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(panel_bg)
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(border_color)
                    .child(
                        Icon::new(IconName::FileCode)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new("Scripts")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(self.render_controls(window, cx))
            .child(
                v_flex()
                    .id("script-list-container")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .border_b_1()
                    .border_color(border_color)
                    .child(self.render_script_list(window, cx)),
            )
            .child(self.render_output(window, cx))
    }
}

impl Focusable for ScriptPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ScriptPanel {
    fn persistent_name() -> &'static str {
        "Script Panel"
    }

    fn panel_key() -> &'static str {
        SCRIPT_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Bottom
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(
            position,
            DockPosition::Left | DockPosition::Right | DockPosition::Bottom
        )
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(300.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::FileCode)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Script Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        20
    }
}
