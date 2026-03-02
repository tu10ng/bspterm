pub mod call_graph;
pub mod code_search;
pub mod code_server_config;
pub mod language;
pub mod parser;
pub mod pipeline;
pub mod renderer;

use std::collections::HashMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use call_graph::{CallEdge, CallGraph, CallGraphNode, DirectlyFollowsGraph};
pub use code_search::{CodeLocation, CodeSource, FunctionLocation, SshDockerCodeSource};
pub use code_server_config::CodeServerConfig;
pub use language::{FunctionDef, LanguageAnalyzer, LanguageRegistry};
pub use parser::{LogEntry, LogParseRule, LogParseRuleStore};
pub use pipeline::{AnalysisContext, AnalysisPipeline, AnalysisStep};
pub use renderer::{render_dot, DotOptions, GraphRenderer};

pub type TraceId = Uuid;
pub type SpanId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: TraceId,
    pub name: String,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub spans: Vec<Span>,
    pub root_spans: Vec<SpanId>,
}

impl Trace {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Local::now();
        Self {
            trace_id: Uuid::new_v4(),
            name: name.into(),
            start_time: now,
            end_time: now,
            spans: Vec::new(),
            root_spans: Vec::new(),
        }
    }

    pub fn add_span(&mut self, span: Span) {
        if span.parent_span_id.is_none() {
            self.root_spans.push(span.span_id);
        }
        if span.start_time < self.start_time {
            self.start_time = span.start_time;
        }
        if let Some(end) = span.end_time {
            if end > self.end_time {
                self.end_time = end;
            }
        }
        self.spans.push(span);
    }

    pub fn get_span(&self, span_id: SpanId) -> Option<&Span> {
        self.spans.iter().find(|s| s.span_id == span_id)
    }

    pub fn get_children(&self, span_id: SpanId) -> Vec<&Span> {
        self.spans
            .iter()
            .filter(|s| s.parent_span_id == Some(span_id))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub trace_id: TraceId,
    pub operation_name: String,
    pub kind: SpanKind,
    pub start_time: DateTime<Local>,
    pub end_time: Option<DateTime<Local>>,
    pub duration: Option<std::time::Duration>,
    pub code_location: Option<CodeLocation>,
    pub attributes: HashMap<String, AttributeValue>,
    pub events: Vec<SpanEvent>,
    pub status: SpanStatus,
    pub log_line_numbers: Vec<usize>,
}

impl Span {
    pub fn new(
        trace_id: TraceId,
        operation_name: impl Into<String>,
        kind: SpanKind,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            span_id: Uuid::new_v4(),
            parent_span_id: None,
            trace_id,
            operation_name: operation_name.into(),
            kind,
            start_time: timestamp,
            end_time: None,
            duration: None,
            code_location: None,
            attributes: HashMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
            log_line_numbers: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: SpanId) -> Self {
        self.parent_span_id = Some(parent_id);
        self
    }

    pub fn with_line_number(mut self, line: usize) -> Self {
        self.log_line_numbers.push(line);
        self
    }

    pub fn set_end(&mut self, end_time: DateTime<Local>) {
        self.end_time = Some(end_time);
        self.duration = Some(
            (end_time - self.start_time)
                .to_std()
                .unwrap_or(std::time::Duration::ZERO),
        );
    }

    pub fn add_attribute(&mut self, key: impl Into<String>, value: AttributeValue) {
        self.attributes.insert(key.into(), value);
    }

    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    Entry,
    Internal,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: DateTime<Local>,
    pub attributes: HashMap<String, AttributeValue>,
}

impl SpanEvent {
    pub fn new(name: impl Into<String>, timestamp: DateTime<Local>) -> Self {
        Self {
            name: name.into(),
            timestamp,
            attributes: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringArray(Vec<String>),
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        AttributeValue::String(s)
    }
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        AttributeValue::String(s.to_string())
    }
}

impl From<i64> for AttributeValue {
    fn from(i: i64) -> Self {
        AttributeValue::Int(i)
    }
}

impl From<f64> for AttributeValue {
    fn from(f: f64) -> Self {
        AttributeValue::Float(f)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        AttributeValue::Bool(b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub trace_flags: TraceFlags,
    pub trace_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TraceFlags {
    pub sampled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextInferenceStrategy {
    IndentBased,
    MarkerBased,
    TimeOverlap,
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextInferenceConfig {
    pub prefer_indent: bool,
    pub use_entry_exit_markers: bool,
    pub use_time_overlap: bool,
    pub verify_with_code: bool,
    pub entry_markers: Vec<String>,
    pub exit_markers: Vec<String>,
    pub time_window_ms: Option<u64>,
}

impl Default for ContextInferenceConfig {
    fn default() -> Self {
        Self {
            prefer_indent: true,
            use_entry_exit_markers: true,
            use_time_overlap: true,
            verify_with_code: false,
            entry_markers: vec![
                ">>>".to_string(),
                "ENTER".to_string(),
                "calling".to_string(),
                "invoke".to_string(),
            ],
            exit_markers: vec![
                "<<<".to_string(),
                "EXIT".to_string(),
                "returning".to_string(),
                "return".to_string(),
            ],
            time_window_ms: Some(100),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilterConfig {
    pub exclude_levels: Vec<String>,
    pub exclude_keywords: Vec<String>,
    pub include_modules: Option<Vec<String>>,
}

impl Default for LogFilterConfig {
    fn default() -> Self {
        Self {
            exclude_levels: vec![
                "DEBUG".to_string(),
                "TRACE".to_string(),
                "HEARTBEAT".to_string(),
            ],
            exclude_keywords: Vec::new(),
            include_modules: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id_pattern: Option<String>,
    pub group_by_keyword: Option<String>,
    pub filter_session: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_id_pattern: None,
            group_by_keyword: None,
            filter_session: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisProgress {
    Idle,
    Parsing {
        current: usize,
        total: usize,
    },
    Searching {
        current: usize,
        total: usize,
    },
    Building,
    Rendering,
    Complete,
    Error,
}

impl Default for AnalysisProgress {
    fn default() -> Self {
        Self::Idle
    }
}
