use anyhow::Result;

use super::{AnalysisContext, AnalysisStep};
use crate::call_graph::CallGraphBuilder;
use crate::{AnalysisProgress, Span, SpanId, SpanKind, Trace};

pub struct GraphBuildStep {
    use_parent_inference: bool,
}

impl GraphBuildStep {
    pub fn new() -> Self {
        Self {
            use_parent_inference: true,
        }
    }

    pub fn without_parent_inference(mut self) -> Self {
        self.use_parent_inference = false;
        self
    }

    fn infer_parent_span(
        &self,
        current_idx: usize,
        spans: &[Span],
        ctx: &AnalysisContext,
    ) -> Option<SpanId> {
        if !self.use_parent_inference || spans.is_empty() {
            return None;
        }

        let current = &ctx.entries.get(current_idx)?;

        if ctx.inference_config.prefer_indent && current.indent_level > 0 {
            for i in (0..spans.len()).rev() {
                let prev_entry = ctx.entries.get(i)?;
                if prev_entry.indent_level < current.indent_level {
                    return Some(spans[i].span_id);
                }
            }
        }

        if ctx.inference_config.use_entry_exit_markers {
            let msg = &current.raw_line;

            for marker in &ctx.inference_config.entry_markers {
                if msg.contains(marker) {
                    if !spans.is_empty() {
                        return Some(spans[spans.len() - 1].span_id);
                    }
                }
            }
        }

        None
    }

    fn entry_to_span(&self, entry: &crate::parser::LogEntry, trace_id: uuid::Uuid) -> Span {
        let kind = self.infer_span_kind(entry);
        let timestamp = entry.timestamp.unwrap_or_else(chrono::Local::now);

        let operation_name = entry
            .function_names
            .first()
            .cloned()
            .unwrap_or_else(|| format!("line_{}", entry.line_number));

        let mut span = Span::new(trace_id, operation_name, kind, timestamp)
            .with_line_number(entry.line_number);

        for (key, value) in &entry.attributes {
            span.add_attribute(key.clone(), value.clone().into());
        }

        span
    }

    fn infer_span_kind(&self, entry: &crate::parser::LogEntry) -> SpanKind {
        let msg = &entry.raw_line.to_lowercase();

        for marker in [">>>", "enter", "start", "begin", "invoke", "calling"] {
            if msg.contains(marker) {
                return SpanKind::Entry;
            }
        }

        for marker in ["<<<", "exit", "end", "return", "complete", "finish"] {
            if msg.contains(marker) {
                return SpanKind::Exit;
            }
        }

        SpanKind::Internal
    }
}

impl Default for GraphBuildStep {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisStep for GraphBuildStep {
    fn name(&self) -> &str {
        "graph_build"
    }

    fn process(&self, ctx: &mut AnalysisContext) -> Result<()> {
        if ctx.entries.is_empty() {
            log::info!("[GraphBuildStep] No entries to process");
            ctx.trace = Some(Trace::new("empty"));
            return Ok(());
        }

        log::info!(
            "[GraphBuildStep] Building graph from {} entries",
            ctx.entries.len()
        );

        ctx.progress = AnalysisProgress::Building;

        let mut trace = Trace::new("log_analysis");
        let trace_id = trace.trace_id;

        let mut spans = Vec::new();

        for (idx, entry) in ctx.entries.iter().enumerate() {
            if entry.function_names.is_empty() {
                continue;
            }

            let mut span = self.entry_to_span(entry, trace_id);

            if let Some(parent_id) = self.infer_parent_span(idx, &spans, ctx) {
                span.parent_span_id = Some(parent_id);
            }

            if let Some(func_name) = entry.function_names.first() {
                if let Some(location) = ctx.locations.get(func_name) {
                    span.code_location = Some(location.clone());
                }
            }

            spans.push(span);
        }

        for span in spans {
            trace.add_span(span);
        }

        log::info!(
            "[GraphBuildStep] Created trace with {} spans, {} roots",
            trace.spans.len(),
            trace.root_spans.len()
        );

        let builder = CallGraphBuilder::new(trace.clone()).with_locations(ctx.locations.clone());
        let graph = builder.build();

        log::info!(
            "[GraphBuildStep] Built call graph with {} nodes and {} edges",
            graph.node_count(),
            graph.edge_count()
        );

        ctx.trace = Some(trace);
        ctx.graph = Some(graph);
        ctx.spans = ctx.trace.as_ref().unwrap().spans.clone();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::LogEntry;

    #[test]
    fn test_graph_build_step() {
        let mut ctx = AnalysisContext::new("test".to_string());

        let mut entry1 = LogEntry::new(1, ">>> process_data");
        entry1.add_function_name("process_data");
        entry1.indent_level = 0;

        let mut entry2 = LogEntry::new(2, "  >>> validate");
        entry2.add_function_name("validate");
        entry2.indent_level = 2;

        ctx.entries.push(entry1);
        ctx.entries.push(entry2);

        let step = GraphBuildStep::new();
        step.process(&mut ctx).unwrap();

        assert!(ctx.trace.is_some());
        assert!(ctx.graph.is_some());

        let graph = ctx.graph.as_ref().unwrap();
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn test_infer_span_kind() {
        let step = GraphBuildStep::new();

        let entry_enter = LogEntry::new(1, ">>> calling function");
        assert_eq!(step.infer_span_kind(&entry_enter), SpanKind::Entry);

        let entry_exit = LogEntry::new(2, "<<< returning from function");
        assert_eq!(step.infer_span_kind(&entry_exit), SpanKind::Exit);

        let entry_internal = LogEntry::new(3, "processing data");
        assert_eq!(step.infer_span_kind(&entry_internal), SpanKind::Internal);
    }
}
