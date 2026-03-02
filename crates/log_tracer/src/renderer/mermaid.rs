use std::collections::HashMap;

use anyhow::Result;
use petgraph::visit::EdgeRef;

use super::{GraphRenderer, RenderOptions};
use crate::CallGraph;

pub struct MermaidRenderer;

impl MermaidRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MermaidRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphRenderer for MermaidRenderer {
    fn format_id(&self) -> &str {
        "mermaid"
    }

    fn render(&self, graph: &CallGraph, options: &RenderOptions) -> Result<String> {
        Ok(render_mermaid(graph, options))
    }
}

pub fn render_mermaid(graph: &CallGraph, options: &RenderOptions) -> String {
    let mut output = String::new();

    let direction = match options.direction {
        super::GraphDirection::TopToBottom => "TB",
        super::GraphDirection::LeftToRight => "LR",
        super::GraphDirection::BottomToTop => "BT",
        super::GraphDirection::RightToLeft => "RL",
    };

    output.push_str(&format!("flowchart {}\n", direction));

    let mut node_ids: HashMap<String, String> = HashMap::new();

    for (idx, node) in graph.nodes().enumerate() {
        let node_id = format!("N{}", idx);
        node_ids.insert(node.name.clone(), node_id.clone());

        let mut label = node.name.clone();
        if label.len() > options.max_label_length {
            label = format!("{}...", &label[..options.max_label_length - 3]);
        }

        if options.show_line_numbers {
            if let Some(ref loc) = node.code_location {
                let file_name = loc.file.rsplit('/').next().unwrap_or(&loc.file);
                label = format!("{}\\n{}:{}", label, file_name, loc.start_line);
            }
        }

        if options.show_call_count && node.call_count > 1 {
            label = format!("{}\\n(×{})", label, node.call_count);
        }

        output.push_str(&format!("    {}[\"{}\"]\n", node_id, escape_mermaid(&label)));
    }

    output.push('\n');

    for edge_ref in graph.graph.edge_references() {
        let source = &graph.graph[edge_ref.source()];
        let target = &graph.graph[edge_ref.target()];
        let edge = edge_ref.weight();

        let from_id = node_ids.get(&source.name).unwrap();
        let to_id = node_ids.get(&target.name).unwrap();

        let edge_label = if options.show_call_count && edge.call_count > 1 {
            format!(" -->|x{}| ", edge.call_count)
        } else if !edge.sequences.is_empty() && edge.sequences.len() <= 3 {
            let seq_str = edge
                .sequences
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(" -->|#{}| ", seq_str)
        } else {
            " --> ".to_string()
        };

        output.push_str(&format!("    {}{}{}\n", from_id, edge_label, to_id));
    }

    if options.highlight_branches {
        output.push('\n');
        for edge_ref in graph.graph.edge_references() {
            let edge = edge_ref.weight();
            if edge.is_branch {
                let source = &graph.graph[edge_ref.source()];
                let from_id = node_ids.get(&source.name).unwrap();
                output.push_str(&format!(
                    "    style {} fill:{}\n",
                    from_id, options.colors.branch_node
                ));
            }
        }
    }

    log::info!(
        "[MermaidRenderer] Generated Mermaid output, {} bytes",
        output.len()
    );

    output
}

fn escape_mermaid(s: &str) -> String {
    s.replace('"', "'")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallEdge, CallGraphNode, Span, SpanKind, Trace};
    use chrono::Local;

    #[test]
    fn test_render_mermaid() {
        let mut trace = Trace::new("test");
        let now = Local::now();
        trace.add_span(Span::new(trace.trace_id, "main", SpanKind::Entry, now));

        let mut graph = CallGraph::new(trace);
        let n1 = graph.add_node(CallGraphNode::new("main"));
        let n2 = graph.add_node(CallGraphNode::new("process"));

        graph.add_edge(n1, n2, CallEdge::new(1));

        let options = RenderOptions::default();
        let mermaid = render_mermaid(&graph, &options);

        assert!(mermaid.contains("flowchart TB"));
        assert!(mermaid.contains("main"));
        assert!(mermaid.contains("process"));
        assert!(mermaid.contains("-->"));
    }
}
