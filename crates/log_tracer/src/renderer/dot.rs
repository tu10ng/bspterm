use std::collections::HashMap;

use anyhow::Result;
use petgraph::visit::EdgeRef;

use super::{GraphRenderer, RenderOptions};
use crate::{CallGraph, SpanStatus};

pub struct DotRenderer;

impl DotRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DotRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphRenderer for DotRenderer {
    fn format_id(&self) -> &str {
        "dot"
    }

    fn render(&self, graph: &CallGraph, options: &RenderOptions) -> Result<String> {
        let dot_options = DotOptions::from_render_options(options);
        Ok(render_dot(graph, &dot_options))
    }
}

#[derive(Debug, Clone)]
pub struct DotOptions {
    pub rankdir: String,
    pub node_shape: String,
    pub node_style: String,
    pub normal_color: String,
    pub branch_color: String,
    pub error_color: String,
    pub entry_color: String,
    pub exit_color: String,
    pub edge_color: String,
    pub branch_edge_color: String,
    pub show_line_numbers: bool,
    pub show_call_count: bool,
    pub show_duration: bool,
    pub highlight_branches: bool,
    pub max_label_length: usize,
    pub font_name: String,
    pub font_size: u32,
}

impl Default for DotOptions {
    fn default() -> Self {
        Self {
            rankdir: "TB".to_string(),
            node_shape: "box".to_string(),
            node_style: "rounded,filled".to_string(),
            normal_color: "#e3f2fd".to_string(),
            branch_color: "#fff3e0".to_string(),
            error_color: "#ffebee".to_string(),
            entry_color: "#e8f5e9".to_string(),
            exit_color: "#fce4ec".to_string(),
            edge_color: "#666666".to_string(),
            branch_edge_color: "#ff9800".to_string(),
            show_line_numbers: true,
            show_call_count: true,
            show_duration: false,
            highlight_branches: true,
            max_label_length: 30,
            font_name: "Helvetica".to_string(),
            font_size: 11,
        }
    }
}

impl DotOptions {
    pub fn from_render_options(options: &RenderOptions) -> Self {
        Self {
            rankdir: options.direction.to_dot_rankdir().to_string(),
            normal_color: options.colors.normal_node.clone(),
            branch_color: options.colors.branch_node.clone(),
            error_color: options.colors.error_node.clone(),
            entry_color: options.colors.entry_node.clone(),
            exit_color: options.colors.exit_node.clone(),
            edge_color: options.colors.normal_edge.clone(),
            branch_edge_color: options.colors.branch_edge.clone(),
            show_line_numbers: options.show_line_numbers,
            show_call_count: options.show_call_count,
            show_duration: options.show_duration,
            highlight_branches: options.highlight_branches,
            max_label_length: options.max_label_length,
            ..Default::default()
        }
    }
}

pub fn render_dot(graph: &CallGraph, options: &DotOptions) -> String {
    let mut dot = String::new();

    dot.push_str("digraph CallGraph {\n");
    dot.push_str(&format!("  rankdir={};\n", options.rankdir));
    dot.push_str(&format!(
        "  node [shape={}, style=\"{}\", fontname=\"{}\", fontsize={}];\n",
        options.node_shape, options.node_style, options.font_name, options.font_size
    ));
    dot.push_str(&format!(
        "  edge [fontname=\"{}\", fontsize={}];\n",
        options.font_name,
        options.font_size - 1
    ));
    dot.push_str("  \n");

    let mut node_ids: HashMap<String, usize> = HashMap::new();
    let mut idx = 0;

    for node in graph.nodes() {
        node_ids.insert(node.name.clone(), idx);

        let label = build_node_label(node, options);
        let color = get_node_color(node, graph, options);

        dot.push_str(&format!(
            "  n{} [label=\"{}\", fillcolor=\"{}\"];\n",
            idx,
            escape_dot_string(&label),
            color
        ));

        idx += 1;
    }

    dot.push_str("  \n");

    for edge_ref in graph.graph.edge_references() {
        let source = &graph.graph[edge_ref.source()];
        let target = &graph.graph[edge_ref.target()];
        let edge = edge_ref.weight();

        let from_idx = node_ids.get(&source.name).unwrap();
        let to_idx = node_ids.get(&target.name).unwrap();

        let mut attrs = Vec::new();

        if options.show_call_count && edge.call_count > 1 {
            attrs.push(format!("label=\"x{}\"", edge.call_count));
        } else if !edge.sequences.is_empty() {
            let seq_str = edge
                .sequences
                .iter()
                .take(3)
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if edge.sequences.len() > 3 {
                attrs.push(format!("label=\"#{}...\"", seq_str));
            } else {
                attrs.push(format!("label=\"#{}\"", seq_str));
            }
        }

        if options.highlight_branches && edge.is_branch {
            attrs.push(format!("color=\"{}\"", options.branch_edge_color));
            attrs.push("penwidth=2".to_string());
        } else {
            attrs.push(format!("color=\"{}\"", options.edge_color));
        }

        let attrs_str = if attrs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", attrs.join(", "))
        };

        dot.push_str(&format!("  n{} -> n{}{};\n", from_idx, to_idx, attrs_str));
    }

    dot.push_str("}\n");

    log::info!(
        "[DotRenderer] Generated DOT output, {} bytes",
        dot.len()
    );

    dot
}

fn build_node_label(
    node: &crate::CallGraphNode,
    options: &DotOptions,
) -> String {
    let mut parts = Vec::new();

    let name = if node.name.len() > options.max_label_length {
        format!("{}...", &node.name[..options.max_label_length - 3])
    } else {
        node.name.clone()
    };
    parts.push(name);

    if options.show_line_numbers {
        if let Some(ref loc) = node.code_location {
            let file_name = loc.file.rsplit('/').next().unwrap_or(&loc.file);
            parts.push(format!("{}:{}", file_name, loc.start_line));
        }
    }

    if options.show_call_count && node.call_count > 1 {
        parts.push(format!("(×{})", node.call_count));
    }

    if options.show_duration && !node.total_duration.is_zero() {
        let ms = node.total_duration.as_millis();
        if ms > 0 {
            parts.push(format!("{}ms", ms));
        }
    }

    parts.join("\\n")
}

fn get_node_color(
    node: &crate::CallGraphNode,
    graph: &CallGraph,
    options: &DotOptions,
) -> String {
    if node.status == SpanStatus::Error {
        return options.error_color.clone();
    }

    if graph.trace.root_spans.iter().any(|id| node.span_id == Some(*id)) {
        return options.entry_color.clone();
    }

    let is_leaf = graph
        .graph
        .node_indices()
        .find(|idx| graph.graph[*idx].name == node.name)
        .map(|idx| graph.graph.edges(idx).next().is_none())
        .unwrap_or(false);

    if is_leaf {
        return options.exit_color.clone();
    }

    let is_branch_source = graph
        .graph
        .node_indices()
        .find(|idx| graph.graph[*idx].name == node.name)
        .map(|idx| {
            graph
                .graph
                .edges(idx)
                .any(|e| e.weight().is_branch)
        })
        .unwrap_or(false);

    if options.highlight_branches && is_branch_source {
        return options.branch_color.clone();
    }

    options.normal_color.clone()
}

fn escape_dot_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallEdge, CallGraphNode, Span, SpanKind, Trace};
    use chrono::Local;

    #[test]
    fn test_render_dot() {
        let mut trace = Trace::new("test");
        let now = Local::now();
        trace.add_span(Span::new(trace.trace_id, "main", SpanKind::Entry, now));

        let mut graph = CallGraph::new(trace);
        let n1 = graph.add_node(CallGraphNode::new("main"));
        let n2 = graph.add_node(CallGraphNode::new("process"));
        let n3 = graph.add_node(CallGraphNode::new("validate"));

        graph.add_edge(n1, n2, CallEdge::new(1));
        graph.add_edge(n2, n3, CallEdge::new(2));

        let options = DotOptions::default();
        let dot = render_dot(&graph, &options);

        assert!(dot.contains("digraph CallGraph"));
        assert!(dot.contains("main"));
        assert!(dot.contains("process"));
        assert!(dot.contains("validate"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_escape_dot_string() {
        assert_eq!(escape_dot_string("test"), "test");
        assert_eq!(escape_dot_string("hello\"world"), "hello\\\"world");
        assert_eq!(escape_dot_string("line1\\nline2"), "line1\\\\nline2");
    }

    #[test]
    fn test_build_node_label() {
        let options = DotOptions::default();
        let node = CallGraphNode::new("process_data");

        let label = build_node_label(&node, &options);
        assert!(label.contains("process_data"));
    }
}
