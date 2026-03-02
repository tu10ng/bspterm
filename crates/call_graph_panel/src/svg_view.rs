use gpui::{Context, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window};
use ui::{prelude::*, v_flex};

pub struct SvgView {
    svg_content: Option<String>,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl SvgView {
    pub fn new() -> Self {
        Self {
            svg_content: None,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    pub fn set_svg(&mut self, svg: String) {
        self.svg_content = Some(svg);
        self.scale = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    pub fn clear(&mut self) {
        self.svg_content = None;
    }

    pub fn zoom_in(&mut self) {
        self.scale = (self.scale * 1.2).min(5.0);
    }

    pub fn zoom_out(&mut self) {
        self.scale = (self.scale / 1.2).max(0.1);
    }

    pub fn reset_zoom(&mut self) {
        self.scale = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset_x += dx;
        self.offset_y += dy;
    }
}

impl Default for SvgView {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for SvgView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("svg-view")
            .size_full()
            .overflow_y_scroll()
            .child(
                if let Some(ref _svg) = self.svg_content {
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .child(
                            ui::Label::new("SVG rendering placeholder")
                                .size(ui::LabelSize::Small)
                                .color(ui::Color::Muted),
                        )
                } else {
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .child(
                            ui::Label::new("No graph to display")
                                .size(ui::LabelSize::Default)
                                .color(ui::Color::Muted),
                        )
                },
            )
    }
}
