use editor::{Editor, EditorEvent};
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window, px,
};
use i18n::t;
use regex::Regex;
use terminal::{
    HighlightProtocol, HighlightRule, HighlightStoreEntity, TerminalTokenModifiers,
    TerminalTokenType,
};
use ui::{prelude::*, Button, ButtonCommon, ButtonStyle, Checkbox, Label, LabelSize, h_flex, v_flex};
use uuid::Uuid;
use workspace::ModalView;

pub struct HighlightEditModal {
    highlight_store: Entity<HighlightStoreEntity>,
    editing_rule_id: Option<Uuid>,
    name_editor: Entity<Editor>,
    pattern_editor: Entity<Editor>,
    sample_text_editor: Entity<Editor>,
    priority_editor: Entity<Editor>,
    foreground_editor: Entity<Editor>,
    background_editor: Entity<Editor>,
    token_type: TerminalTokenType,
    protocol: HighlightProtocol,
    modifiers: TerminalTokenModifiers,
    case_insensitive: bool,
    regex_error: Option<String>,
    match_count: usize,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl HighlightEditModal {
    pub fn new_rule(
        highlight_store: Entity<HighlightStoreEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("highlight_edit.name_placeholder"), window, cx);
            editor
        });

        let pattern_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("highlight_edit.pattern_placeholder"), window, cx);
            editor
        });

        let sample_text_editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text(&t("highlight_edit.sample_placeholder"), window, cx);
            editor.set_text(
                "2024-01-01 ERROR: Connection failed\nWarning: deprecated function\nfatal error occurred",
                window,
                cx,
            );
            editor
        });

        let priority_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text("50", window, cx);
            editor
        });

        let foreground_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("#ff0000", window, cx);
            editor
        });

        let background_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("highlight_edit.none"), window, cx);
            editor
        });

        let pattern_subscription =
            cx.subscribe(&pattern_editor, |this, _, _event: &EditorEvent, cx| {
                this.validate_and_update_preview(cx);
            });

        let sample_subscription =
            cx.subscribe(&sample_text_editor, |this, _, _event: &EditorEvent, cx| {
                this.validate_and_update_preview(cx);
            });

        Self {
            highlight_store,
            editing_rule_id: None,
            name_editor,
            pattern_editor,
            sample_text_editor,
            priority_editor,
            foreground_editor,
            background_editor,
            token_type: TerminalTokenType::Error,
            protocol: HighlightProtocol::All,
            modifiers: TerminalTokenModifiers::new(),
            case_insensitive: true,
            regex_error: None,
            match_count: 0,
            focus_handle,
            _subscriptions: vec![pattern_subscription, sample_subscription],
        }
    }

    pub fn edit_rule(
        highlight_store: Entity<HighlightStoreEntity>,
        rule: HighlightRule,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(rule.name.clone(), window, cx);
            editor.set_placeholder_text(&t("highlight_edit.name_placeholder"), window, cx);
            editor
        });

        let pattern_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(rule.pattern.clone(), window, cx);
            editor.set_placeholder_text(&t("highlight_edit.pattern_placeholder"), window, cx);
            editor
        });

        let sample_text_editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text(&t("highlight_edit.sample_placeholder"), window, cx);
            editor.set_text(
                "2024-01-01 ERROR: Connection failed\nWarning: deprecated function\nfatal error occurred",
                window,
                cx,
            );
            editor
        });

        let priority_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(rule.priority.to_string(), window, cx);
            editor
        });

        let foreground_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            if let Some(color) = &rule.foreground_color {
                editor.set_text(color.clone(), window, cx);
            }
            editor.set_placeholder_text("#ff0000", window, cx);
            editor
        });

        let background_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            if let Some(color) = &rule.background_color {
                editor.set_text(color.clone(), window, cx);
            }
            editor.set_placeholder_text(&t("highlight_edit.none"), window, cx);
            editor
        });

        let pattern_subscription =
            cx.subscribe(&pattern_editor, |this, _, _event: &EditorEvent, cx| {
                this.validate_and_update_preview(cx);
            });

        let sample_subscription =
            cx.subscribe(&sample_text_editor, |this, _, _event: &EditorEvent, cx| {
                this.validate_and_update_preview(cx);
            });

        let mut modal = Self {
            highlight_store,
            editing_rule_id: Some(rule.id),
            name_editor,
            pattern_editor,
            sample_text_editor,
            priority_editor,
            foreground_editor,
            background_editor,
            token_type: rule.token_type,
            protocol: rule.protocol,
            modifiers: rule.modifiers,
            case_insensitive: rule.case_insensitive,
            regex_error: None,
            match_count: 0,
            focus_handle,
            _subscriptions: vec![pattern_subscription, sample_subscription],
        };

        modal.validate_and_update_preview(cx);
        modal
    }

    fn validate_and_update_preview(&mut self, cx: &mut Context<Self>) {
        let pattern = self.pattern_editor.read(cx).text(cx);
        let sample = self.sample_text_editor.read(cx).text(cx);

        if pattern.is_empty() {
            self.regex_error = None;
            self.match_count = 0;
            cx.notify();
            return;
        }

        let regex_pattern = if self.case_insensitive {
            format!("(?i){}", pattern)
        } else {
            pattern
        };

        match Regex::new(&regex_pattern) {
            Ok(regex) => {
                self.regex_error = None;
                self.match_count = regex.find_iter(&sample).count();
            }
            Err(err) => {
                self.regex_error = Some(format!("{}", err));
                self.match_count = 0;
            }
        }

        cx.notify();
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_editor.read(cx).text(cx);
        let pattern = self.pattern_editor.read(cx).text(cx);
        let priority_str = self.priority_editor.read(cx).text(cx);
        let foreground = self.foreground_editor.read(cx).text(cx);
        let background = self.background_editor.read(cx).text(cx);

        let priority: i32 = priority_str.parse().unwrap_or(50);

        let foreground_color = if foreground.trim().is_empty() {
            None
        } else {
            Some(foreground.trim().to_string())
        };

        let background_color = if background.trim().is_empty() {
            None
        } else {
            Some(background.trim().to_string())
        };

        if let Some(id) = self.editing_rule_id {
            let token_type = self.token_type.clone();
            let protocol = self.protocol.clone();
            let modifiers = self.modifiers;
            let case_insensitive = self.case_insensitive;

            self.highlight_store.update(cx, |store, cx| {
                store.update_rule(
                    id,
                    |rule| {
                        rule.name = name;
                        rule.pattern = pattern;
                        rule.token_type = token_type;
                        rule.protocol = protocol;
                        rule.modifiers = modifiers;
                        rule.case_insensitive = case_insensitive;
                        rule.priority = priority;
                        rule.foreground_color = foreground_color;
                        rule.background_color = background_color;
                    },
                    cx,
                );
            });
        } else {
            let rule = HighlightRule::new(name, pattern, self.token_type.clone())
                .with_protocol(self.protocol.clone())
                .with_modifiers(self.modifiers)
                .with_case_insensitive(self.case_insensitive)
                .with_priority(priority);

            let rule = if let Some(color) = foreground_color {
                rule.with_foreground_color(color)
            } else {
                rule
            };

            let rule = if let Some(color) = background_color {
                rule.with_background_color(color)
            } else {
                rule
            };

            self.highlight_store.update(cx, |store, cx| {
                store.add_rule(rule, cx);
            });
        }

        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn set_token_type(&mut self, token_type: TerminalTokenType, cx: &mut Context<Self>) {
        self.token_type = token_type;
        cx.notify();
    }

    fn set_protocol(&mut self, protocol: HighlightProtocol, cx: &mut Context<Self>) {
        self.protocol = protocol;
        cx.notify();
    }

    fn toggle_modifier(&mut self, modifier: u32, cx: &mut Context<Self>) {
        self.modifiers.0 ^= modifier;
        cx.notify();
    }

    fn toggle_case_insensitive(&mut self, cx: &mut Context<Self>) {
        self.case_insensitive = !self.case_insensitive;
        self.validate_and_update_preview(cx);
    }

    fn render_token_type_button(
        &self,
        id: &'static str,
        label: &'static str,
        token_type: TerminalTokenType,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.token_type == token_type;
        Button::new(id, label)
            .style(if is_selected {
                ButtonStyle::Filled
            } else {
                ButtonStyle::Subtle
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.set_token_type(token_type.clone(), cx);
            }))
    }

    fn render_protocol_button(
        &self,
        id: &'static str,
        label: &'static str,
        protocol: HighlightProtocol,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.protocol == protocol;
        Button::new(id, label)
            .style(if is_selected {
                ButtonStyle::Filled
            } else {
                ButtonStyle::Subtle
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.set_protocol(protocol.clone(), cx);
            }))
    }
}

impl Render for HighlightEditModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.editing_rule_id.is_some() {
            t("highlight_edit.title_edit")
        } else {
            t("highlight_edit.title_new")
        };

        let regex_valid = self.regex_error.is_none();
        let match_count = self.match_count;
        let case_insensitive = self.case_insensitive;

        v_flex()
            .id("highlight-edit-modal")
            .key_context("HighlightEditModal")
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .p_4()
            .gap_3()
            .w(px(500.))
            .max_h(px(700.))
            .overflow_y_scroll()
            .child(Label::new(title).size(LabelSize::Large))
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("common.name")).size(LabelSize::Small))
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.name_editor.clone()),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(Label::new(t("highlight_edit.pattern")).size(LabelSize::Small))
                            .child(if regex_valid {
                                Label::new(t("highlight_edit.valid"))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Success)
                                    .into_any_element()
                            } else {
                                Label::new(t("highlight_edit.invalid"))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Error)
                                    .into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .border_1()
                            .border_color(if regex_valid {
                                cx.theme().colors().border
                            } else {
                                cx.theme().colors().border_variant
                            })
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .child(self.pattern_editor.clone()),
                    )
                    .when_some(self.regex_error.clone(), |this, error| {
                        this.child(
                            Label::new(error)
                                .size(LabelSize::XSmall)
                                .color(Color::Error),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("highlight_edit.token_type")).size(LabelSize::Small))
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(self.render_token_type_button(
                                "token-error",
                                "Error",
                                TerminalTokenType::Error,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-warning",
                                "Warning",
                                TerminalTokenType::Warning,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-info",
                                "Info",
                                TerminalTokenType::Info,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-debug",
                                "Debug",
                                TerminalTokenType::Debug,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-success",
                                "Success",
                                TerminalTokenType::Success,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-timestamp",
                                "Timestamp",
                                TerminalTokenType::Timestamp,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-ip",
                                "IP",
                                TerminalTokenType::IpAddress,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-url",
                                "URL",
                                TerminalTokenType::Url,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-path",
                                "Path",
                                TerminalTokenType::Path,
                                cx,
                            ))
                            .child(self.render_token_type_button(
                                "token-number",
                                "Number",
                                TerminalTokenType::Number,
                                cx,
                            )),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("highlight_edit.protocol")).size(LabelSize::Small))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(self.render_protocol_button(
                                "protocol-all",
                                "All",
                                HighlightProtocol::All,
                                cx,
                            ))
                            .child(self.render_protocol_button(
                                "protocol-ssh",
                                "SSH",
                                HighlightProtocol::Ssh,
                                cx,
                            ))
                            .child(self.render_protocol_button(
                                "protocol-telnet",
                                "Telnet",
                                HighlightProtocol::Telnet,
                                cx,
                            ))
                            .child(self.render_protocol_button(
                                "protocol-local",
                                "Local",
                                HighlightProtocol::Local,
                                cx,
                            )),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(t("highlight_edit.modifiers")).size(LabelSize::Small))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Checkbox::new(
                                    "modifier-bold",
                                    self.modifiers.is_bold().into(),
                                )
                                .label("Bold")
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_modifier(TerminalTokenModifiers::BOLD, cx);
                                })),
                            )
                            .child(
                                Checkbox::new(
                                    "modifier-italic",
                                    self.modifiers.is_italic().into(),
                                )
                                .label("Italic")
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_modifier(TerminalTokenModifiers::ITALIC, cx);
                                })),
                            )
                            .child(
                                Checkbox::new(
                                    "modifier-underline",
                                    self.modifiers.is_underline().into(),
                                )
                                .label("Underline")
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_modifier(TerminalTokenModifiers::UNDERLINE, cx);
                                })),
                            )
                            .child(
                                Checkbox::new(
                                    "modifier-dim",
                                    self.modifiers.is_dim().into(),
                                )
                                .label("Dim")
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_modifier(TerminalTokenModifiers::DIM, cx);
                                })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(Label::new(t("highlight_edit.priority")).size(LabelSize::Small))
                            .child(
                                div()
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .rounded_md()
                                    .px_2()
                                    .py_1()
                                    .child(self.priority_editor.clone()),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(
                                Label::new(t("highlight_edit.case_insensitive"))
                                    .size(LabelSize::Small),
                            )
                            .child(
                                Checkbox::new("case-insensitive", case_insensitive.into())
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_case_insensitive(cx);
                                    })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(
                                Label::new(t("highlight_edit.foreground_color"))
                                    .size(LabelSize::Small),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex_1()
                                            .border_1()
                                            .border_color(cx.theme().colors().border)
                                            .rounded_md()
                                            .px_2()
                                            .py_1()
                                            .child(self.foreground_editor.clone()),
                                    )
                                    .child(render_color_preview_from_editor(
                                        &self.foreground_editor,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(
                                Label::new(t("highlight_edit.background_color"))
                                    .size(LabelSize::Small),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex_1()
                                            .border_1()
                                            .border_color(cx.theme().colors().border)
                                            .rounded_md()
                                            .px_2()
                                            .py_1()
                                            .child(self.background_editor.clone()),
                                    )
                                    .child(render_color_preview_from_editor(
                                        &self.background_editor,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(Label::new(t("highlight_edit.preview")).size(LabelSize::Small))
                            .child(
                                Label::new(format!(
                                    "{} {}",
                                    match_count,
                                    t("highlight_edit.matches")
                                ))
                                .size(LabelSize::XSmall)
                                .color(if match_count > 0 {
                                    Color::Success
                                } else {
                                    Color::Muted
                                }),
                            ),
                    )
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .h(px(80.))
                            .child(self.sample_text_editor.clone()),
                    ),
            )
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
                        Button::new("save", t("common.save"))
                            .style(ButtonStyle::Filled)
                            .disabled(!regex_valid)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}

fn render_color_preview_from_editor(
    editor: &Entity<Editor>,
    cx: &Context<HighlightEditModal>,
) -> impl IntoElement {
    let text = editor.read(cx).text(cx);
    let color = if text.trim().is_empty() {
        None
    } else {
        parse_hex_color(&text)
    };

    div()
        .w(px(24.))
        .h(px(24.))
        .rounded(px(4.))
        .border_1()
        .border_color(cx.theme().colors().border)
        .when_some(color, |this, c| this.bg(c))
}

fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(gpui::hsla(
        rgb_to_hue(r, g, b),
        rgb_to_saturation(r, g, b),
        rgb_to_lightness(r, g, b),
        1.0,
    ))
}

fn rgb_to_hue(r: u8, g: u8, b: u8) -> f32 {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    if delta == 0.0 {
        return 0.0;
    }

    let hue = if max == r {
        ((g - b) / delta) % 6.0
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };

    (hue * 60.0 / 360.0).rem_euclid(1.0)
}

fn rgb_to_saturation(r: u8, g: u8, b: u8) -> f32 {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if max == min {
        0.0
    } else {
        let delta = max - min;
        if l > 0.5 {
            delta / (2.0 - max - min)
        } else {
            delta / (max + min)
        }
    }
}

fn rgb_to_lightness(r: u8, g: u8, b: u8) -> f32 {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    (max + min) / 2.0
}

impl Focusable for HighlightEditModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for HighlightEditModal {}

impl ModalView for HighlightEditModal {}
