use std::collections::HashMap;
use std::path::PathBuf;

use editor::actions::{Backtab, Tab};
use editor::Editor;
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, Styled, Window, px,
};
use i18n::t;
use menu;
use ui::{prelude::*, Button, ButtonStyle, Checkbox, Label, LabelSize, h_flex, v_flex};
use workspace::ModalView;

use crate::script_params::{ParamType, ScriptParam, ScriptParams};

pub type OnRunCallback = Box<dyn FnOnce(PathBuf, HashMap<String, String>, &mut App)>;

pub struct ScriptParamsModal {
    script_name: String,
    script_path: PathBuf,
    script_params: ScriptParams,
    on_run: Option<OnRunCallback>,
    editors: Vec<Entity<Editor>>,
    checkbox_states: Vec<bool>,
    choice_selections: Vec<usize>,
    focus_handle: FocusHandle,
    initial_focus: FocusHandle,
}

impl ScriptParamsModal {
    pub fn new(
        script_name: String,
        script_path: PathBuf,
        script_params: ScriptParams,
        on_run: OnRunCallback,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let mut editors = Vec::new();
        let mut checkbox_states = Vec::new();
        let mut choice_selections = Vec::new();

        for param in &script_params.params {
            match param.param_type {
                ParamType::Boolean => {
                    let default_val = param
                        .default
                        .as_ref()
                        .map(|s| s.to_lowercase() == "true")
                        .unwrap_or(false);
                    checkbox_states.push(default_val);
                    editors.push(cx.new(|cx| Editor::single_line(window, cx)));
                    choice_selections.push(0);
                }
                ParamType::Choice => {
                    let default_idx = param
                        .default
                        .as_ref()
                        .and_then(|d| {
                            param
                                .choices
                                .as_ref()
                                .and_then(|choices| choices.iter().position(|c| c == d))
                        })
                        .unwrap_or(0);
                    choice_selections.push(default_idx);
                    checkbox_states.push(false);
                    editors.push(cx.new(|cx| Editor::single_line(window, cx)));
                }
                _ => {
                    let editor = cx.new(|cx| {
                        let mut editor = Editor::single_line(window, cx);
                        if let Some(default) = &param.default {
                            editor.set_text(default.clone(), window, cx);
                        }
                        if let Some(desc) = &param.description {
                            editor.set_placeholder_text(desc, window, cx);
                        }
                        editor
                    });
                    editors.push(editor);
                    checkbox_states.push(false);
                    choice_selections.push(0);
                }
            }
        }

        let initial_focus = script_params
            .params
            .iter()
            .position(|p| !matches!(p.param_type, ParamType::Boolean | ParamType::Choice))
            .map(|idx| editors[idx].focus_handle(cx))
            .unwrap_or_else(|| focus_handle.clone());

        Self {
            script_name,
            script_path,
            script_params,
            on_run: Some(on_run),
            editors,
            checkbox_states,
            choice_selections,
            focus_handle,
            initial_focus,
        }
    }

    fn can_run(&self, cx: &App) -> bool {
        for (i, param) in self.script_params.params.iter().enumerate() {
            if param.required {
                match param.param_type {
                    ParamType::Boolean => {}
                    ParamType::Choice => {}
                    _ => {
                        let text = self.editors[i].read(cx).text(cx);
                        if text.trim().is_empty() {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn collect_values(&self, cx: &App) -> HashMap<String, String> {
        let mut values = HashMap::new();

        for (i, param) in self.script_params.params.iter().enumerate() {
            let value = match param.param_type {
                ParamType::Boolean => {
                    if self.checkbox_states[i] {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                ParamType::Choice => {
                    if let Some(choices) = &param.choices {
                        let idx = self.choice_selections[i];
                        choices.get(idx).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    }
                }
                _ => self.editors[i].read(cx).text(cx).to_string(),
            };
            values.insert(param.name.clone(), value);
        }

        values
    }

    fn run(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_run(cx) {
            return;
        }

        let values = self.collect_values(cx);
        let env_params = self.script_params.to_env_map(&values);
        let script_path = self.script_path.clone();

        if let Some(on_run) = self.on_run.take() {
            on_run(script_path, env_params, cx);
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn focusable_editor_indices(&self) -> Vec<usize> {
        self.script_params
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| !matches!(p.param_type, ParamType::Boolean | ParamType::Choice))
            .map(|(i, _)| i)
            .collect()
    }

    fn focus_next_param(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        let indices = self.focusable_editor_indices();
        if indices.len() <= 1 {
            return;
        }
        if let Some(current) = indices
            .iter()
            .position(|&i| self.editors[i].focus_handle(cx).is_focused(window))
        {
            let next = (current + 1) % indices.len();
            window.focus(&self.editors[indices[next]].focus_handle(cx), cx);
            cx.stop_propagation();
        }
    }

    fn focus_prev_param(&mut self, _: &Backtab, window: &mut Window, cx: &mut Context<Self>) {
        let indices = self.focusable_editor_indices();
        if indices.len() <= 1 {
            return;
        }
        if let Some(current) = indices
            .iter()
            .position(|&i| self.editors[i].focus_handle(cx).is_focused(window))
        {
            let prev = if current == 0 {
                indices.len() - 1
            } else {
                current - 1
            };
            window.focus(&self.editors[indices[prev]].focus_handle(cx), cx);
            cx.stop_propagation();
        }
    }

    fn toggle_checkbox(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.checkbox_states.len() {
            self.checkbox_states[index] = !self.checkbox_states[index];
            cx.notify();
        }
    }

    fn set_choice(&mut self, param_index: usize, choice_index: usize, cx: &mut Context<Self>) {
        if param_index < self.choice_selections.len() {
            self.choice_selections[param_index] = choice_index;
            cx.notify();
        }
    }

    fn render_params(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.script_params
            .params
            .iter()
            .enumerate()
            .map(|(i, param)| self.render_param(i, param, cx).into_any_element())
            .collect()
    }

    fn render_param(
        &self,
        index: usize,
        param: &ScriptParam,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = if let Some(desc) = &param.description {
            desc.clone()
        } else {
            param.name.clone()
        };

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(Label::new(label).size(LabelSize::Small))
                    .when(param.required, |this| {
                        this.child(
                            Label::new("*")
                                .size(LabelSize::Small)
                                .color(Color::Error),
                        )
                    }),
            )
            .child(self.render_param_input(index, param, cx))
    }

    fn render_param_input(
        &self,
        index: usize,
        param: &ScriptParam,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        match param.param_type {
            ParamType::Boolean => {
                let checked = self.checkbox_states[index];
                div().child(
                    Checkbox::new(format!("param-checkbox-{}", index), checked.into()).on_click(
                        cx.listener(move |this, _, _window, cx| {
                            this.toggle_checkbox(index, cx);
                        }),
                    ),
                )
            }
            ParamType::Choice => {
                let choices = param.choices.clone().unwrap_or_default();
                let selected = self.choice_selections[index];

                let choice_buttons: Vec<_> = choices
                    .into_iter()
                    .enumerate()
                    .map(|(choice_idx, choice)| {
                        let is_selected = choice_idx == selected;
                        Button::new(format!("choice-{}-{}", index, choice_idx), choice)
                            .style(if is_selected {
                                ButtonStyle::Filled
                            } else {
                                ButtonStyle::Subtle
                            })
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.set_choice(index, choice_idx, cx);
                            }))
                    })
                    .collect();

                div().child(h_flex().gap_1().flex_wrap().children(choice_buttons))
            }
            _ => div()
                .border_1()
                .border_color(cx.theme().colors().border)
                .rounded_md()
                .px_2()
                .py_1()
                .child(self.editors[index].clone()),
        }
    }
}

impl Render for ScriptParamsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = format!("{}: {}", t("script_params.title"), &self.script_name);
        let can_run = self.can_run(cx);
        let param_elements = self.render_params(cx);

        v_flex()
            .id("script-params-modal")
            .key_context("ScriptParamsModal")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                this.run(window, cx);
            }))
            .capture_action(cx.listener(Self::focus_next_param))
            .capture_action(cx.listener(Self::focus_prev_param))
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(450.))
            .max_h(px(600.))
            .overflow_y_scroll()
            .child(Label::new(title).size(LabelSize::Large))
            .children(param_elements)
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .mt_2()
                    .child(
                        Button::new("cancel", t("common.cancel"))
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("run", t("script_params.run_button"))
                            .style(ButtonStyle::Filled)
                            .disabled(!can_run)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(window, cx);
                            })),
                    ),
            )
    }
}

impl Focusable for ScriptParamsModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.initial_focus.clone()
    }
}

impl EventEmitter<DismissEvent> for ScriptParamsModal {}

impl ModalView for ScriptParamsModal {}
