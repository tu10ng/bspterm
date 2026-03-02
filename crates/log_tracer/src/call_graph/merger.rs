use super::{CallEdge, CallGraph, CallGraphNode};
use crate::Trace;

pub fn merge_graphs(graphs: Vec<CallGraph>) -> CallGraph {
    if graphs.is_empty() {
        return CallGraph::new(Trace::new("empty"));
    }

    if graphs.len() == 1 {
        return graphs.into_iter().next().unwrap();
    }

    let mut traces: Vec<Trace> = graphs.iter().map(|g| g.trace.clone()).collect();
    let mut merged_trace = traces.remove(0);
    for trace in traces {
        for span in trace.spans {
            merged_trace.add_span(span);
        }
    }

    let mut merged = CallGraph::new(merged_trace);

    for graph in graphs {
        for node in graph.nodes() {
            if let Some(existing_idx) = merged.get_node_index(&node.name) {
                if let Some(existing) = merged.graph.node_weight_mut(existing_idx) {
                    existing.call_count += node.call_count;
                    existing.total_duration += node.total_duration;
                    if existing.code_location.is_none() && node.code_location.is_some() {
                        existing.code_location = node.code_location.clone();
                    }
                }
            } else {
                merged.add_node(node.clone());
            }
        }

        for (source, target, edge) in graph.edges() {
            let from_idx = merged.get_node_index(&source.name);
            let to_idx = merged.get_node_index(&target.name);

            if let (Some(from), Some(to)) = (from_idx, to_idx) {
                merged.add_edge(from, to, edge.clone());
            }
        }
    }

    merged.mark_branches();
    merged.compress_paths();

    log::info!(
        "[merger] Merged graphs into {} nodes, {} edges",
        merged.node_count(),
        merged.edge_count()
    );

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Span, SpanKind, Trace};
    use chrono::Local;

    #[test]
    fn test_merge_empty() {
        let result = merge_graphs(vec![]);
        assert_eq!(result.node_count(), 0);
    }

    #[test]
    fn test_merge_single() {
        let mut trace = Trace::new("test");
        let now = Local::now();
        trace.add_span(Span::new(trace.trace_id, "main", SpanKind::Entry, now));

        let mut graph = CallGraph::new(trace);
        graph.add_node(CallGraphNode::new("main"));

        let result = merge_graphs(vec![graph]);
        assert_eq!(result.node_count(), 1);
    }

    #[test]
    fn test_merge_multiple() {
        let now = Local::now();

        let mut trace1 = Trace::new("trace1");
        trace1.add_span(Span::new(trace1.trace_id, "main", SpanKind::Entry, now));
        trace1.add_span(Span::new(trace1.trace_id, "process", SpanKind::Internal, now));

        let mut graph1 = CallGraph::new(trace1);
        let n1 = graph1.add_node(CallGraphNode::new("main"));
        let n2 = graph1.add_node(CallGraphNode::new("process"));
        graph1.add_edge(n1, n2, CallEdge::new(1));

        let mut trace2 = Trace::new("trace2");
        trace2.add_span(Span::new(trace2.trace_id, "process", SpanKind::Internal, now));
        trace2.add_span(Span::new(trace2.trace_id, "validate", SpanKind::Internal, now));

        let mut graph2 = CallGraph::new(trace2);
        let n3 = graph2.add_node(CallGraphNode::new("process"));
        let n4 = graph2.add_node(CallGraphNode::new("validate"));
        graph2.add_edge(n3, n4, CallEdge::new(1));

        let merged = merge_graphs(vec![graph1, graph2]);

        assert_eq!(merged.node_count(), 3);
        assert!(merged.get_node("main").is_some());
        assert!(merged.get_node("process").is_some());
        assert!(merged.get_node("validate").is_some());

        let process_node = merged.get_node("process").unwrap();
        assert_eq!(process_node.call_count, 2);
    }
}
