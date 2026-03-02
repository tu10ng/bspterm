use std::collections::HashMap;

use crate::{CodeLocation, Span, Trace};

use super::{CallEdge, CallGraph, CallGraphNode};

pub struct CallGraphBuilder {
    trace: Trace,
    locations: HashMap<String, CodeLocation>,
}

impl CallGraphBuilder {
    pub fn new(trace: Trace) -> Self {
        Self {
            trace,
            locations: HashMap::new(),
        }
    }

    pub fn with_locations(mut self, locations: HashMap<String, CodeLocation>) -> Self {
        self.locations = locations;
        self
    }

    pub fn build(self) -> CallGraph {
        let mut graph = CallGraph::new(self.trace.clone());

        for span in &self.trace.spans {
            self.add_span_to_graph(&mut graph, span);
        }

        self.add_edges_from_spans(&mut graph);

        graph.mark_branches();
        graph.compress_paths();

        log::info!(
            "[CallGraphBuilder] Built graph with {} nodes and {} edges",
            graph.node_count(),
            graph.edge_count()
        );

        graph
    }

    fn add_span_to_graph(&self, graph: &mut CallGraph, span: &Span) {
        if graph.get_node_index(&span.operation_name).is_some() {
            if let Some(node) = graph.get_node(&span.operation_name).cloned() {
                let idx = graph.get_node_index(&span.operation_name).unwrap();
                if let Some(node_ref) = graph.graph.node_weight_mut(idx) {
                    node_ref.increment_count();
                    if let Some(duration) = span.duration {
                        node_ref.add_duration(duration);
                    }
                }
            }
        } else {
            let mut node = CallGraphNode::from_span(span);

            if let Some(location) = self.locations.get(&span.operation_name) {
                node.code_location = Some(location.clone());
            }

            graph.add_node(node);
        }
    }

    fn add_edges_from_spans(&self, graph: &mut CallGraph) {
        let mut sequence = 0;

        for span in &self.trace.spans {
            if let Some(parent_id) = span.parent_span_id {
                if let Some(parent_span) = self.trace.get_span(parent_id) {
                    let from_idx = graph.get_node_index(&parent_span.operation_name);
                    let to_idx = graph.get_node_index(&span.operation_name);

                    if let (Some(from), Some(to)) = (from_idx, to_idx) {
                        sequence += 1;
                        graph.add_edge(from, to, CallEdge::new(sequence));
                    }
                }
            }
        }

        for window in self.trace.spans.windows(2) {
            let from_span = &window[0];
            let to_span = &window[1];

            if to_span.parent_span_id.is_none() {
                let from_idx = graph.get_node_index(&from_span.operation_name);
                let to_idx = graph.get_node_index(&to_span.operation_name);

                if let (Some(from), Some(to)) = (from_idx, to_idx) {
                    if from != to {
                        sequence += 1;
                        graph.add_edge(from, to, CallEdge::new(sequence));
                    }
                }
            }
        }
    }
}

pub struct IncrementalBuilder {
    graph: CallGraph,
    locations: HashMap<String, CodeLocation>,
    sequence: usize,
    last_span_name: Option<String>,
}

impl IncrementalBuilder {
    pub fn new(trace: Trace) -> Self {
        Self {
            graph: CallGraph::new(trace),
            locations: HashMap::new(),
            sequence: 0,
            last_span_name: None,
        }
    }

    pub fn with_locations(mut self, locations: HashMap<String, CodeLocation>) -> Self {
        self.locations = locations;
        self
    }

    pub fn add_span(&mut self, span: &Span) {
        let node_idx = if let Some(idx) = self.graph.get_node_index(&span.operation_name) {
            if let Some(node) = self.graph.graph.node_weight_mut(idx) {
                node.increment_count();
                if let Some(duration) = span.duration {
                    node.add_duration(duration);
                }
            }
            idx
        } else {
            let mut node = CallGraphNode::from_span(span);
            if let Some(location) = self.locations.get(&span.operation_name) {
                node.code_location = Some(location.clone());
            }
            self.graph.add_node(node)
        };

        if let Some(ref last_name) = self.last_span_name {
            if let Some(last_idx) = self.graph.get_node_index(last_name) {
                if last_idx != node_idx {
                    self.sequence += 1;
                    self.graph
                        .add_edge(last_idx, node_idx, CallEdge::new(self.sequence));
                }
            }
        }

        self.last_span_name = Some(span.operation_name.clone());
    }

    pub fn finish(mut self) -> CallGraph {
        self.graph.mark_branches();
        self.graph.compress_paths();
        self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpanKind;
    use chrono::Local;

    #[test]
    fn test_build_graph() {
        let mut trace = Trace::new("test");
        let now = Local::now();

        let span1 = Span::new(trace.trace_id, "main", SpanKind::Entry, now);
        let span1_id = span1.span_id;

        let span2 = Span::new(trace.trace_id, "process", SpanKind::Internal, now)
            .with_parent(span1_id);

        let span3 = Span::new(trace.trace_id, "validate", SpanKind::Internal, now)
            .with_parent(span1_id);

        trace.add_span(span1);
        trace.add_span(span2);
        trace.add_span(span3);

        let builder = CallGraphBuilder::new(trace);
        let graph = builder.build();

        assert_eq!(graph.node_count(), 3);
        assert!(graph.get_node("main").is_some());
        assert!(graph.get_node("process").is_some());
        assert!(graph.get_node("validate").is_some());
    }

    #[test]
    fn test_incremental_builder() {
        let trace = Trace::new("test");
        let now = Local::now();

        let mut builder = IncrementalBuilder::new(trace);

        let span1 = Span::new(uuid::Uuid::new_v4(), "main", SpanKind::Entry, now);
        let span2 = Span::new(uuid::Uuid::new_v4(), "process", SpanKind::Internal, now);
        let span3 = Span::new(uuid::Uuid::new_v4(), "cleanup", SpanKind::Exit, now);

        builder.add_span(&span1);
        builder.add_span(&span2);
        builder.add_span(&span3);

        let graph = builder.finish();

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }
}
