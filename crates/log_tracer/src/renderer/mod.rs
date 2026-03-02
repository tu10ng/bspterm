mod dot;
mod mermaid;

pub use dot::{render_dot, DotOptions, DotRenderer};
pub use mermaid::MermaidRenderer;

use anyhow::Result;

use crate::CallGraph;

pub trait GraphRenderer: Send + Sync {
    fn format_id(&self) -> &str;

    fn render(&self, graph: &CallGraph, options: &RenderOptions) -> Result<String>;
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub show_line_numbers: bool,
    pub show_call_count: bool,
    pub show_duration: bool,
    pub highlight_branches: bool,
    pub colors: ColorScheme,
    pub compress_paths: bool,
    pub min_frequency: usize,
    pub max_label_length: usize,
    pub direction: GraphDirection,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            show_call_count: true,
            show_duration: false,
            highlight_branches: true,
            colors: ColorScheme::default(),
            compress_paths: true,
            min_frequency: 1,
            max_label_length: 30,
            direction: GraphDirection::TopToBottom,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub normal_node: String,
    pub branch_node: String,
    pub error_node: String,
    pub entry_node: String,
    pub exit_node: String,
    pub normal_edge: String,
    pub branch_edge: String,
    pub background: String,
    pub text: String,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            normal_node: "#e3f2fd".to_string(),
            branch_node: "#fff3e0".to_string(),
            error_node: "#ffebee".to_string(),
            entry_node: "#e8f5e9".to_string(),
            exit_node: "#fce4ec".to_string(),
            normal_edge: "#666666".to_string(),
            branch_edge: "#ff9800".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
        }
    }
}

impl ColorScheme {
    pub fn dark() -> Self {
        Self {
            normal_node: "#1e3a5f".to_string(),
            branch_node: "#4a3728".to_string(),
            error_node: "#4a2828".to_string(),
            entry_node: "#2a4a2a".to_string(),
            exit_node: "#4a2a3a".to_string(),
            normal_edge: "#888888".to_string(),
            branch_edge: "#ffb74d".to_string(),
            background: "#1e1e1e".to_string(),
            text: "#e0e0e0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDirection {
    TopToBottom,
    LeftToRight,
    BottomToTop,
    RightToLeft,
}

impl GraphDirection {
    pub fn to_dot_rankdir(&self) -> &'static str {
        match self {
            Self::TopToBottom => "TB",
            Self::LeftToRight => "LR",
            Self::BottomToTop => "BT",
            Self::RightToLeft => "RL",
        }
    }
}
