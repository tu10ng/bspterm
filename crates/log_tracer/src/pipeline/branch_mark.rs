use anyhow::Result;

use super::{AnalysisContext, AnalysisStep};

pub struct BranchMarkStep;

impl BranchMarkStep {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BranchMarkStep {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisStep for BranchMarkStep {
    fn name(&self) -> &str {
        "branch_mark"
    }

    fn process(&self, ctx: &mut AnalysisContext) -> Result<()> {
        if let Some(ref mut graph) = ctx.graph {
            log::info!("[BranchMarkStep] Marking branch paths in graph");

            graph.mark_branches();
            graph.compress_paths();

            let branch_count = graph
                .graph
                .edge_weights()
                .filter(|e| e.is_branch)
                .count();

            log::info!(
                "[BranchMarkStep] Marked {} branch edges",
                branch_count
            );
        } else {
            log::info!("[BranchMarkStep] No graph to process");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_graph::{CallEdge, CallGraph, CallGraphNode};
    use crate::{Span, SpanKind, Trace};
    use chrono::Local;

    #[test]
    fn test_branch_mark_step() {
        let now = Local::now();
        let mut trace = Trace::new("test");
        trace.add_span(Span::new(trace.trace_id, "main", SpanKind::Entry, now));

        let mut graph = CallGraph::new(trace);
        let n1 = graph.add_node(CallGraphNode::new("main"));
        let n2 = graph.add_node(CallGraphNode::new("branch_a"));
        let n3 = graph.add_node(CallGraphNode::new("branch_b"));

        graph.add_edge(n1, n2, CallEdge::new(1));
        graph.add_edge(n1, n3, CallEdge::new(2));

        let mut ctx = AnalysisContext::new("test".to_string());
        ctx.graph = Some(graph);

        let step = BranchMarkStep::new();
        step.process(&mut ctx).unwrap();

        let graph = ctx.graph.as_ref().unwrap();
        let branch_count = graph
            .graph
            .edge_weights()
            .filter(|e| e.is_branch)
            .count();

        assert_eq!(branch_count, 2);
    }
}
