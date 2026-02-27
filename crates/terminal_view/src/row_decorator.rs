use chrono::{DateTime, TimeDelta, Local};
use gpui::{Hsla, Pixels, px};
use terminal::Terminal;

/// A decoration item to paint for a row
#[derive(Debug, Clone)]
pub enum RowDecoration {
    /// Horizontal separator line (drawn above the row)
    Separator { color: Hsla, thickness: Pixels },
}

/// Trait for row decoration painters
pub trait RowDecorator: Send + Sync {
    /// Compute decorations for a given row during layout phase
    /// Returns decorations to draw before this row
    fn layout(
        &mut self,
        row_index: usize,
        grid_line: i32,
        terminal: &Terminal,
    ) -> Vec<RowDecoration>;

    /// Reset state (called at start of each layout pass)
    fn reset(&mut self);
}

/// Decorator that draws separators when timestamp gap exceeds threshold
pub struct TimestampGapDecorator {
    threshold: TimeDelta,
    prev_timestamp: Option<DateTime<Local>>,
    separator_color: Hsla,
}

impl TimestampGapDecorator {
    pub fn new(threshold_seconds: i64, separator_color: Hsla) -> Self {
        Self {
            threshold: TimeDelta::seconds(threshold_seconds),
            prev_timestamp: None,
            separator_color,
        }
    }
}

impl RowDecorator for TimestampGapDecorator {
    fn reset(&mut self) {
        self.prev_timestamp = None;
    }

    fn layout(
        &mut self,
        _row_index: usize,
        grid_line: i32,
        terminal: &Terminal,
    ) -> Vec<RowDecoration> {
        let mut items = Vec::new();

        if let Some(curr_ts) = terminal.get_line_timestamp(grid_line) {
            if let Some(prev_ts) = self.prev_timestamp {
                let diff = curr_ts.signed_duration_since(prev_ts);
                if diff > self.threshold {
                    items.push(RowDecoration::Separator {
                        color: self.separator_color,
                        thickness: px(1.),
                    });
                }
            }
            self.prev_timestamp = Some(curr_ts);
        }

        items
    }
}
