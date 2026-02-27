mod highlight_edit_modal;

use anyhow::Result;
use bspterm_actions::highlight_editor::ToggleFocus;
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, ListSizingBehavior, ParentElement, Render, Styled,
    Subscription, UniformListScrollHandle, WeakEntity, Window, px, uniform_list,
};
use i18n::t;
use terminal::{HighlightRule, HighlightStoreEntity, HighlightStoreEvent};
use ui::{
    prelude::*, Button, ButtonCommon, ButtonStyle, Checkbox, Color, Icon, IconName, IconSize,
    Label, LabelSize, ListItem, ListItemSpacing, Tooltip, h_flex, v_flex,
};
use uuid::Uuid;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

pub use highlight_edit_modal::HighlightEditModal;

const HIGHLIGHT_EDITOR_PANEL_KEY: &str = "HighlightEditorPanel";

pub fn init(cx: &mut App) {
    HighlightStoreEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<HighlightEditor>(window, cx);
        });
    })
    .detach();
}

pub struct HighlightEditor {
    highlight_store: Entity<HighlightStoreEntity>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    rules: Vec<HighlightRule>,
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    selected_rule_id: Option<Uuid>,
    _subscriptions: Vec<Subscription>,
}

impl HighlightEditor {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let highlight_store = HighlightStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let store_subscription =
            cx.subscribe(&highlight_store, |this, _, event, cx| match event {
                HighlightStoreEvent::Changed
                | HighlightStoreEvent::RuleAdded(_)
                | HighlightStoreEvent::RuleRemoved(_)
                | HighlightStoreEvent::HighlightingToggled(_) => {
                    this.update_rules(cx);
                }
            });

        let mut this = Self {
            highlight_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            rules: Vec::new(),
            workspace: weak_workspace,
            width: None,
            selected_rule_id: None,
            _subscriptions: vec![store_subscription],
        };

        this.update_rules(cx);
        this
    }

    fn update_rules(&mut self, cx: &mut Context<Self>) {
        self.rules = self.highlight_store.read(cx).rules().to_vec();
        cx.notify();
    }

    fn toggle_highlighting(&mut self, cx: &mut Context<Self>) {
        self.highlight_store.update(cx, |store, cx| {
            store.toggle_highlighting(cx);
        });
    }

    fn toggle_rule_enabled(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.highlight_store.update(cx, |store, cx| {
            store.toggle_rule_enabled(id, cx);
        });
    }

    fn delete_rule(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.highlight_store.update(cx, |store, cx| {
            store.remove_rule(id, cx);
        });
    }

    fn reset_to_defaults(&mut self, cx: &mut Context<Self>) {
        self.highlight_store.update(cx, |store, cx| {
            store.reset_to_defaults(cx);
        });
    }

    fn add_new_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let highlight_store = self.highlight_store.clone();
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    HighlightEditModal::new_rule(highlight_store, window, cx)
                });
            });
        }
    }

    fn edit_rule(&mut self, rule: &HighlightRule, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let highlight_store = self.highlight_store.clone();
            let rule = rule.clone();
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    HighlightEditModal::edit_rule(highlight_store, rule, window, cx)
                });
            });
        }
    }

    fn render_entries(
        &self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter_map(|ix| self.rules.get(ix).map(|rule| self.render_rule_item(rule, cx)))
            .collect()
    }

    fn render_rule_item(&self, rule: &HighlightRule, cx: &mut Context<Self>) -> AnyElement {
        let id = rule.id;
        let name = rule.name.clone();
        let enabled = rule.enabled;
        let pattern = truncate_string(&rule.pattern, 25);
        let token_type_label = rule.token_type.label().to_string();
        let protocol_label = rule.protocol.label().to_string();
        let fg_color = rule.foreground_color.clone();

        ListItem::new(ElementId::from(SharedString::from(format!(
            "highlight-rule-{}",
            id
        ))))
        .spacing(ListItemSpacing::Sparse)
        .start_slot(
            Checkbox::new(
                SharedString::from(format!("highlight-checkbox-{}", id)),
                enabled.into(),
            )
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.toggle_rule_enabled(id, cx);
            })),
        )
        .child(
            v_flex()
                .gap_0p5()
                .overflow_hidden()
                .child(Label::new(name).size(LabelSize::Default))
                .child(
                    h_flex()
                        .gap_1()
                        .overflow_hidden()
                        .child(
                            Label::new(pattern)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format!("[{}]", token_type_label))
                                .size(LabelSize::XSmall)
                                .color(Color::Accent),
                        )
                        .child(
                            Label::new(format!("[{}]", protocol_label))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .when_some(fg_color, |this, color| {
                    this.child(
                        h_flex().gap_1().child(render_color_swatch(&color, cx)).child(
                            Label::new(color)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                    )
                }),
        )
        .end_slot(
            h_flex()
                .gap_1()
                .child(
                    Button::new(
                        SharedString::from(format!("edit-highlight-{}", id)),
                        t("common.edit"),
                    )
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener({
                        let rule = rule.clone();
                        move |this, _, window, cx| {
                            this.edit_rule(&rule, window, cx);
                        }
                    })),
                )
                .child(
                    Button::new(
                        SharedString::from(format!("delete-highlight-{}", id)),
                        t("common.delete"),
                    )
                    .style(ButtonStyle::Subtle)
                    .color(Color::Error)
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.delete_rule(id, cx);
                    })),
                ),
        )
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.selected_rule_id = Some(id);
            cx.notify();
        }))
        .into_any_element()
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

fn render_color_swatch(hex_color: &str, cx: &Context<HighlightEditor>) -> impl IntoElement {
    let color = parse_hex_color(hex_color).unwrap_or(gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    });

    div()
        .w(px(12.))
        .h(px(12.))
        .rounded(px(2.))
        .bg(color)
        .border_1()
        .border_color(cx.theme().colors().border)
}

fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim_start_matches('#');
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

impl Render for HighlightEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rule_count = self.rules.len();
        let highlighting_enabled = self.highlight_store.read(cx).highlighting_enabled();

        v_flex()
            .key_context("HighlightEditor")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_1()
                            .child(Icon::new(IconName::Sparkle).size(IconSize::Small))
                            .child(
                                Label::new(t("highlight_editor.title")).size(LabelSize::Default),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Checkbox::new("highlight-master-toggle", highlighting_enabled.into())
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_highlighting(cx);
                                    })),
                            )
                            .child(
                                Button::new("add-highlight-rule", "")
                                    .style(ButtonStyle::Subtle)
                                    .icon(IconName::Plus)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text(t("highlight_editor.add_rule")))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_new_rule(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Button::new(
                            "reset-highlight-defaults",
                            t("highlight_editor.reset_defaults"),
                        )
                        .style(ButtonStyle::Subtle)
                        .icon(IconName::RotateCcw)
                        .icon_size(IconSize::Small)
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.reset_to_defaults(cx);
                        })),
                    ),
            )
            .child(
                v_flex()
                    .flex_grow()
                    .child(if rule_count > 0 {
                        uniform_list(
                            "highlight-rules-list",
                            rule_count,
                            cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                                this.render_entries(range, window, cx)
                            }),
                        )
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&self.scroll_handle)
                        .into_any_element()
                    } else {
                        v_flex()
                            .size_full()
                            .justify_center()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Sparkle)
                                    .size(IconSize::Medium)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(t("highlight_editor.no_rules"))
                                    .size(LabelSize::Default)
                                    .color(Color::Muted),
                            )
                            .into_any_element()
                    }),
            )
    }
}

impl Focusable for HighlightEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for HighlightEditor {}

impl Panel for HighlightEditor {
    fn persistent_name() -> &'static str {
        "Highlight Editor"
    }

    fn panel_key() -> &'static str {
        HIGHLIGHT_EDITOR_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(320.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Sparkle)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("highlight_editor.title")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        12
    }
}
