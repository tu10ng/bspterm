use gpui::{
    App, Context, DragMoveEvent, ElementId, IntoElement, MouseButton, ParentElement, Pixels,
    Point, Render, RenderOnce, Styled, Window, div, px,
};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use terminal::ParsedNumber;
use ui::{
    prelude::*, Color, CopyButton, FluentBuilder, IconButton, IconName, IconSize, Label,
    LabelCommon, Tooltip, h_flex, v_flex,
};

static NEXT_POPOVER_ID: AtomicUsize = AtomicUsize::new(0);

fn next_popover_id() -> usize {
    NEXT_POPOVER_ID.fetch_add(1, Ordering::SeqCst)
}

/// Drag handle for moving pinned popovers.
#[derive(Debug, Clone)]
pub struct NumberPopoverDrag {
    pub popover_id: usize,
    pub initial_position: Point<Pixels>,
}

/// View shown while dragging a number popover.
struct DraggedPopoverView;

impl Render for DraggedPopoverView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .opacity(0.7)
            .bg(cx.theme().colors().surface_background)
            .rounded_md()
            .p_2()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(Label::new("Moving...").size(LabelSize::Small))
    }
}

/// State for a number popover showing binary/decimal/hex conversion.
#[derive(Debug, Clone)]
pub struct NumberPopover {
    pub id: usize,
    pub parsed: ParsedNumber,
    pub is_pinned: bool,
    pub position: Point<Pixels>,
}

impl NumberPopover {
    pub fn new(parsed: ParsedNumber, position: Point<Pixels>) -> Self {
        Self {
            id: next_popover_id(),
            parsed,
            is_pinned: false,
            position,
        }
    }

    pub fn toggle_pinned(&mut self) {
        self.is_pinned = !self.is_pinned;
    }
}

/// Props for rendering a number popover.
#[derive(IntoElement)]
pub struct NumberPopoverElement {
    popover: NumberPopover,
    on_pin: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_close: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,
    on_drag_move: Option<Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App) + 'static>>,
}

impl NumberPopoverElement {
    pub fn new(popover: NumberPopover) -> Self {
        Self {
            popover,
            on_pin: None,
            on_close: None,
            on_drag_move: None,
        }
    }

    pub fn on_pin(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_pin = Some(Rc::new(handler));
        self
    }

    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    pub fn on_drag_move(
        mut self,
        handler: impl Fn(Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drag_move = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for NumberPopoverElement {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let popover_id = self.popover.id;
        let is_pinned = self.popover.is_pinned;
        let parsed = self.popover.parsed.clone();
        let position = self.popover.position;

        let (binary_str, bit_positions) = parsed.format_as_binary();
        let decimal_str = parsed.format_as_decimal();
        let hex_str = parsed.format_as_hex();

        let pin_icon = IconName::Pin;

        let on_pin = self.on_pin.clone();
        let on_close = self.on_close.clone();
        let on_drag_move = self.on_drag_move.clone();

        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .pb_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(format!("Original: {}", parsed.original))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child({
                        let on_pin = on_pin.clone();
                        IconButton::new(
                            ElementId::Name(format!("pin-{}", popover_id).into()),
                            pin_icon,
                        )
                        .icon_size(IconSize::XSmall)
                        .icon_color(if is_pinned {
                            Color::Accent
                        } else {
                            Color::Muted
                        })
                        .tooltip(Tooltip::text(if is_pinned { "Unpin" } else { "Pin" }))
                        .on_click(move |_, window, cx| {
                            if let Some(ref handler) = on_pin {
                                handler(window, cx);
                            }
                        })
                    })
                    .when(is_pinned, |this| {
                        let on_close = on_close.clone();
                        this.child(
                            IconButton::new(
                                ElementId::Name(format!("close-{}", popover_id).into()),
                                IconName::Close,
                            )
                            .icon_size(IconSize::XSmall)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Close"))
                            .on_click(move |_, window, cx| {
                                if let Some(ref handler) = on_close {
                                    handler(window, cx);
                                }
                            }),
                        )
                    }),
            );

        let binary_row = Self::render_format_row(
            "Binary",
            &binary_str,
            Some(&bit_positions),
            popover_id,
            "binary",
            cx,
        );

        let decimal_row =
            Self::render_format_row("Decimal", &decimal_str, None, popover_id, "decimal", cx);

        let hex_row = Self::render_format_row("Hex", &hex_str, None, popover_id, "hex", cx);

        let content = v_flex()
            .gap_1()
            .pt_1()
            .child(binary_row)
            .child(decimal_row)
            .child(hex_row);

        let drag_data = NumberPopoverDrag {
            popover_id,
            initial_position: position,
        };

        v_flex()
            .id(ElementId::Name(
                format!("number-popover-{}", popover_id).into(),
            ))
            .elevation_3(cx)
            .bg(cx.theme().colors().surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_md()
            .p_2()
            .min_w(px(280.))
            .max_w(px(450.))
            .cursor_default()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .when(is_pinned, |this| {
                this.on_drag(drag_data.clone(), |_drag_data, _, _, cx| {
                    cx.new(|_| DraggedPopoverView)
                })
            })
            .when_some(on_drag_move, |this, on_drag_move| {
                this.on_drag_move::<NumberPopoverDrag>(
                    move |event: &DragMoveEvent<NumberPopoverDrag>, window, cx| {
                        if event.bounds.contains(&event.event.position) {
                            on_drag_move(event.event.position, window, cx);
                        }
                    },
                )
            })
            .child(header)
            .child(content)
    }
}

impl NumberPopoverElement {
    fn render_format_row(
        label: &str,
        value: &str,
        subscript: Option<&str>,
        popover_id: usize,
        format_name: &str,
        cx: &App,
    ) -> impl IntoElement {
        let value_string = value.to_string();
        let copy_id = format!("copy-{}-{}", popover_id, format_name);

        h_flex()
            .w_full()
            .justify_between()
            .items_start()
            .gap_2()
            .group("format-row")
            .child(
                h_flex()
                    .min_w(px(60.))
                    .child(Label::new(format!("{}:", label)).size(LabelSize::Small)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .overflow_x_hidden()
                    .child(Label::new(value_string.clone()).size(LabelSize::Small).buffer_font(cx))
                    .when_some(subscript, |this, sub| {
                        this.child(
                            Label::new(sub.to_string())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .buffer_font(cx),
                        )
                    }),
            )
            .child(
                CopyButton::new(ElementId::Name(copy_id.into()), value_string)
                    .icon_size(IconSize::XSmall)
                    .tooltip_label("Copy")
                    .visible_on_hover("format-row"),
            )
    }
}
