mod branch_mark;
mod function_search;
mod graph_build;
mod log_parse;

pub use branch_mark::BranchMarkStep;
pub use function_search::FunctionSearchStep;
pub use graph_build::GraphBuildStep;
pub use log_parse::LogParseStep;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::{
    AnalysisProgress, CallGraph, CodeLocation, CodeSource, ContextInferenceConfig,
    LogFilterConfig, SessionConfig, Span, Trace,
};
use crate::language::LanguageRegistry;
use crate::parser::{CompiledLogRule, LogEntry, LogParseRule};

pub struct AnalysisContext {
    pub log_content: String,
    pub rule: Option<CompiledLogRule>,
    pub entries: Vec<LogEntry>,
    pub spans: Vec<Span>,
    pub trace: Option<Trace>,
    pub graph: Option<CallGraph>,
    pub locations: HashMap<String, CodeLocation>,
    pub code_source: Option<Arc<dyn CodeSource>>,
    pub language_registry: LanguageRegistry,
    pub inference_config: ContextInferenceConfig,
    pub filter_config: LogFilterConfig,
    pub session_config: SessionConfig,
    pub progress: AnalysisProgress,
    pub errors: Vec<String>,
}

impl AnalysisContext {
    pub fn new(log_content: String) -> Self {
        Self {
            log_content,
            rule: None,
            entries: Vec::new(),
            spans: Vec::new(),
            trace: None,
            graph: None,
            locations: HashMap::new(),
            code_source: None,
            language_registry: LanguageRegistry::with_defaults(),
            inference_config: ContextInferenceConfig::default(),
            filter_config: LogFilterConfig::default(),
            session_config: SessionConfig::default(),
            progress: AnalysisProgress::Idle,
            errors: Vec::new(),
        }
    }

    pub fn with_rule(mut self, rule: LogParseRule) -> Result<Self> {
        self.rule = Some(CompiledLogRule::compile(rule)?);
        Ok(self)
    }

    pub fn with_code_source(mut self, source: Arc<dyn CodeSource>) -> Self {
        self.code_source = Some(source);
        self
    }

    pub fn with_inference_config(mut self, config: ContextInferenceConfig) -> Self {
        self.inference_config = config;
        self
    }

    pub fn with_filter_config(mut self, config: LogFilterConfig) -> Self {
        self.filter_config = config;
        self
    }

    pub fn with_session_config(mut self, config: SessionConfig) -> Self {
        self.session_config = config;
        self
    }

    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn unique_function_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .entries
            .iter()
            .flat_map(|e| e.function_names.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

pub trait AnalysisStep: Send + Sync {
    fn name(&self) -> &str;

    fn process(&self, ctx: &mut AnalysisContext) -> Result<()>;
}

pub struct AnalysisPipeline {
    steps: Vec<Box<dyn AnalysisStep>>,
}

impl AnalysisPipeline {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn default_pipeline() -> Self {
        Self::new()
            .add_step(Box::new(LogParseStep::new()))
            .add_step(Box::new(FunctionSearchStep::new()))
            .add_step(Box::new(GraphBuildStep::new()))
            .add_step(Box::new(BranchMarkStep::new()))
    }

    pub fn add_step(mut self, step: Box<dyn AnalysisStep>) -> Self {
        self.steps.push(step);
        self
    }

    pub fn insert_before(
        mut self,
        before: &str,
        step: Box<dyn AnalysisStep>,
    ) -> Self {
        if let Some(pos) = self.steps.iter().position(|s| s.name() == before) {
            self.steps.insert(pos, step);
        } else {
            self.steps.push(step);
        }
        self
    }

    pub fn insert_after(
        mut self,
        after: &str,
        step: Box<dyn AnalysisStep>,
    ) -> Self {
        if let Some(pos) = self.steps.iter().position(|s| s.name() == after) {
            self.steps.insert(pos + 1, step);
        } else {
            self.steps.push(step);
        }
        self
    }

    pub fn remove_step(mut self, name: &str) -> Self {
        self.steps.retain(|s| s.name() != name);
        self
    }

    pub fn run(&self, ctx: &mut AnalysisContext) -> Result<()> {
        log::info!(
            "[AnalysisPipeline] Starting analysis with {} steps",
            self.steps.len()
        );

        for step in &self.steps {
            log::info!("[AnalysisPipeline] Running step: {}", step.name());

            if let Err(e) = step.process(ctx) {
                log::error!(
                    "[AnalysisPipeline] Step '{}' failed: {}",
                    step.name(),
                    e
                );
                ctx.add_error(format!("Step '{}' failed: {}", step.name(), e));
                return Err(e);
            }
        }

        ctx.progress = AnalysisProgress::Complete;
        log::info!("[AnalysisPipeline] Analysis complete");

        Ok(())
    }

    pub fn step_names(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.name()).collect()
    }
}

impl Default for AnalysisPipeline {
    fn default() -> Self {
        Self::default_pipeline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestStep {
        name: String,
    }

    impl AnalysisStep for TestStep {
        fn name(&self) -> &str {
            &self.name
        }

        fn process(&self, ctx: &mut AnalysisContext) -> Result<()> {
            ctx.errors.push(format!("Ran: {}", self.name));
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_order() {
        let pipeline = AnalysisPipeline::new()
            .add_step(Box::new(TestStep {
                name: "step1".to_string(),
            }))
            .add_step(Box::new(TestStep {
                name: "step2".to_string(),
            }))
            .add_step(Box::new(TestStep {
                name: "step3".to_string(),
            }));

        let mut ctx = AnalysisContext::new("test".to_string());
        pipeline.run(&mut ctx).unwrap();

        assert_eq!(ctx.errors.len(), 3);
        assert_eq!(ctx.errors[0], "Ran: step1");
        assert_eq!(ctx.errors[1], "Ran: step2");
        assert_eq!(ctx.errors[2], "Ran: step3");
    }

    #[test]
    fn test_insert_before() {
        let pipeline = AnalysisPipeline::new()
            .add_step(Box::new(TestStep {
                name: "step1".to_string(),
            }))
            .add_step(Box::new(TestStep {
                name: "step3".to_string(),
            }))
            .insert_before(
                "step3",
                Box::new(TestStep {
                    name: "step2".to_string(),
                }),
            );

        let names = pipeline.step_names();
        assert_eq!(names, vec!["step1", "step2", "step3"]);
    }
}
