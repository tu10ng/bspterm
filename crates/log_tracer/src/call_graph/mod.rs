mod builder;
mod merger;

pub use builder::CallGraphBuilder;
pub use merger::merge_graphs;

use std::collections::HashMap;
use std::time::Duration;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::{CodeLocation, Span, SpanId, SpanStatus, Trace};

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub trace: Trace,
    pub graph: DiGraph<CallGraphNode, CallEdge>,
    node_map: HashMap<String, NodeIndex>,
}

impl CallGraph {
    pub fn new(trace: Trace) -> Self {
        Self {
            trace,
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: CallGraphNode) -> NodeIndex {
        let name = node.name.clone();
        let idx = self.graph.add_node(node);
        self.node_map.insert(name, idx);
        idx
    }

    pub fn get_node_index(&self, name: &str) -> Option<NodeIndex> {
        self.node_map.get(name).copied()
    }

    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: CallEdge) {
        if let Some(existing) = self.graph.find_edge(from, to) {
            let weight = self.graph.edge_weight_mut(existing).unwrap();
            weight.call_count += edge.call_count;
            weight.sequences.extend(edge.sequences);
        } else {
            self.graph.add_edge(from, to, edge);
        }
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &CallGraphNode> {
        self.graph.node_weights()
    }

    pub fn edges(&self) -> impl Iterator<Item = (&CallGraphNode, &CallGraphNode, &CallEdge)> {
        self.graph.edge_references().map(|e| {
            let source = &self.graph[e.source()];
            let target = &self.graph[e.target()];
            let weight = e.weight();
            (source, target, weight)
        })
    }

    pub fn get_node(&self, name: &str) -> Option<&CallGraphNode> {
        self.node_map
            .get(name)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    pub fn mark_branches(&mut self) {
        let mut branch_targets: Vec<NodeIndex> = Vec::new();

        for node_idx in self.graph.node_indices() {
            let out_edges: Vec<_> = self.graph.edges(node_idx).collect();
            if out_edges.len() > 1 {
                for edge in out_edges {
                    branch_targets.push(edge.target());
                }
            }
        }

        for target_idx in branch_targets {
            let edge_ids: Vec<_> = self
                .graph
                .edges_directed(target_idx, petgraph::Direction::Incoming)
                .map(|e| e.id())
                .collect();

            for edge_id in edge_ids {
                if let Some(weight) = self.graph.edge_weight_mut(edge_id) {
                    weight.is_branch = true;
                }
            }
        }

        log::info!(
            "[CallGraph] Marked {} branch edges",
            self.graph
                .edge_weights()
                .filter(|e| e.is_branch)
                .count()
        );
    }

    pub fn compress_paths(&mut self) {
        let mut merged = 0;
        let mut edge_counts: HashMap<(NodeIndex, NodeIndex), usize> = HashMap::new();

        for edge in self.graph.edge_references() {
            *edge_counts.entry((edge.source(), edge.target())).or_insert(0) += 1;
        }

        for ((src, dst), count) in edge_counts {
            if count > 1 {
                if let Some(edge_idx) = self.graph.find_edge(src, dst) {
                    if let Some(weight) = self.graph.edge_weight_mut(edge_idx) {
                        weight.call_count = count;
                        merged += count - 1;
                    }
                }
            }
        }

        log::info!("[CallGraph] Path compression: merged {} repeated edges", merged);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphNode {
    pub span_id: Option<SpanId>,
    pub name: String,
    pub code_location: Option<CodeLocation>,
    pub call_count: usize,
    pub total_duration: Duration,
    pub status: SpanStatus,
}

impl CallGraphNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            span_id: None,
            name: name.into(),
            code_location: None,
            call_count: 1,
            total_duration: Duration::ZERO,
            status: SpanStatus::Unset,
        }
    }

    pub fn from_span(span: &Span) -> Self {
        Self {
            span_id: Some(span.span_id),
            name: span.operation_name.clone(),
            code_location: span.code_location.clone(),
            call_count: 1,
            total_duration: span.duration.unwrap_or(Duration::ZERO),
            status: span.status,
        }
    }

    pub fn with_location(mut self, location: CodeLocation) -> Self {
        self.code_location = Some(location);
        self
    }

    pub fn increment_count(&mut self) {
        self.call_count += 1;
    }

    pub fn add_duration(&mut self, duration: Duration) {
        self.total_duration += duration;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub call_count: usize,
    pub sequences: Vec<usize>,
    pub is_branch: bool,
}

impl CallEdge {
    pub fn new(sequence: usize) -> Self {
        Self {
            call_count: 1,
            sequences: vec![sequence],
            is_branch: false,
        }
    }

    pub fn merge(&mut self, other: &CallEdge) {
        self.call_count += other.call_count;
        self.sequences.extend(&other.sequences);
    }
}

impl Default for CallEdge {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone)]
pub struct DirectlyFollowsGraph {
    graph: DiGraph<DfgNode, DfgEdge>,
    node_map: HashMap<String, NodeIndex>,
}

impl DirectlyFollowsGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    pub fn from_spans(spans: &[Span]) -> Self {
        let mut dfg = Self::new();

        for span in spans {
            dfg.add_activity(&span.operation_name, span.code_location.clone());
        }

        for window in spans.windows(2) {
            let from = &window[0].operation_name;
            let to = &window[1].operation_name;
            let duration = window[1].start_time - window[0].start_time;
            dfg.add_edge(from, to, duration.to_std().ok());
        }

        dfg
    }

    pub fn add_activity(&mut self, activity: &str, location: Option<CodeLocation>) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(activity) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.frequency += 1;
            }
            idx
        } else {
            let node = DfgNode {
                activity: activity.to_string(),
                frequency: 1,
                code_location: location,
            };
            let idx = self.graph.add_node(node);
            self.node_map.insert(activity.to_string(), idx);
            idx
        }
    }

    pub fn add_edge(&mut self, from: &str, to: &str, duration: Option<Duration>) {
        let from_idx = self.node_map.get(from).copied();
        let to_idx = self.node_map.get(to).copied();

        if let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) {
            if let Some(edge_idx) = self.graph.find_edge(from_idx, to_idx) {
                let edge = self.graph.edge_weight_mut(edge_idx).unwrap();
                edge.frequency += 1;
                if let Some(d) = duration {
                    let total = edge.avg_duration.unwrap_or(Duration::ZERO) * (edge.frequency - 1) as u32;
                    edge.avg_duration = Some((total + d) / edge.frequency as u32);
                }
            } else {
                self.graph.add_edge(
                    from_idx,
                    to_idx,
                    DfgEdge {
                        frequency: 1,
                        avg_duration: duration,
                        is_branch: false,
                    },
                );
            }
        }
    }

    pub fn filter_infrequent(&mut self, min_frequency: usize) {
        let to_remove: Vec<_> = self
            .graph
            .edge_indices()
            .filter(|&e| self.graph[e].frequency < min_frequency)
            .collect();

        for edge_idx in to_remove.into_iter().rev() {
            self.graph.remove_edge(edge_idx);
        }

        log::info!(
            "[DFG] Filtered edges with frequency < {}, remaining: {}",
            min_frequency,
            self.graph.edge_count()
        );
    }

    pub fn mark_branches(&mut self) {
        for node_idx in self.graph.node_indices() {
            let out_edges: Vec<_> = self.graph.edges(node_idx).map(|e| e.id()).collect();
            if out_edges.len() > 1 {
                for edge_idx in out_edges {
                    if let Some(edge) = self.graph.edge_weight_mut(edge_idx) {
                        edge.is_branch = true;
                    }
                }
            }
        }
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &DfgNode> {
        self.graph.node_weights()
    }

    pub fn edges(&self) -> impl Iterator<Item = (&DfgNode, &DfgNode, &DfgEdge)> {
        self.graph.edge_references().map(|e| {
            let source = &self.graph[e.source()];
            let target = &self.graph[e.target()];
            let weight = e.weight();
            (source, target, weight)
        })
    }
}

impl Default for DirectlyFollowsGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct DfgNode {
    pub activity: String,
    pub frequency: usize,
    pub code_location: Option<CodeLocation>,
}

#[derive(Debug, Clone)]
pub struct DfgEdge {
    pub frequency: usize,
    pub avg_duration: Option<Duration>,
    pub is_branch: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SpanKind, Trace};
    use chrono::Local;

    fn create_test_trace() -> Trace {
        let mut trace = Trace::new("test");
        let now = Local::now();

        let span1 = Span::new(trace.trace_id, "main", SpanKind::Entry, now);
        let span2 = Span::new(trace.trace_id, "process", SpanKind::Internal, now);
        let span3 = Span::new(trace.trace_id, "validate", SpanKind::Internal, now);

        trace.add_span(span1);
        trace.add_span(span2);
        trace.add_span(span3);

        trace
    }

    #[test]
    fn test_call_graph_basic() {
        let trace = create_test_trace();
        let mut graph = CallGraph::new(trace);

        let n1 = graph.add_node(CallGraphNode::new("main"));
        let n2 = graph.add_node(CallGraphNode::new("process"));
        let n3 = graph.add_node(CallGraphNode::new("validate"));

        graph.add_edge(n1, n2, CallEdge::new(1));
        graph.add_edge(n2, n3, CallEdge::new(2));

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_dfg_from_spans() {
        let trace = create_test_trace();
        let dfg = DirectlyFollowsGraph::from_spans(&trace.spans);

        assert_eq!(dfg.node_count(), 3);
        assert_eq!(dfg.edge_count(), 2);
    }

    #[test]
    fn test_mark_branches() {
        let trace = create_test_trace();
        let mut graph = CallGraph::new(trace);

        let n1 = graph.add_node(CallGraphNode::new("main"));
        let n2 = graph.add_node(CallGraphNode::new("branch_a"));
        let n3 = graph.add_node(CallGraphNode::new("branch_b"));

        graph.add_edge(n1, n2, CallEdge::new(1));
        graph.add_edge(n1, n3, CallEdge::new(2));

        graph.mark_branches();

        let branch_count = graph.graph.edge_weights().filter(|e| e.is_branch).count();
        assert_eq!(branch_count, 2);
    }
}
