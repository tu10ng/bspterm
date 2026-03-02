mod keyword_matcher;
mod rule_engine;

pub use keyword_matcher::KeywordMatcher;
pub use rule_engine::{CompiledLogRule, LogParseRule, LogParseRuleStore};

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub line_number: usize,
    pub timestamp: Option<DateTime<Local>>,
    pub level: Option<String>,
    pub module: Option<String>,
    pub message: String,
    pub function_names: Vec<String>,
    pub indent_level: usize,
    pub raw_line: String,
    pub attributes: HashMap<String, String>,
}

impl LogEntry {
    pub fn new(line_number: usize, raw_line: impl Into<String>) -> Self {
        let raw = raw_line.into();
        let indent_level = count_indent(&raw);
        Self {
            line_number,
            timestamp: None,
            level: None,
            module: None,
            message: raw.trim().to_string(),
            function_names: Vec::new(),
            indent_level,
            raw_line: raw,
            attributes: HashMap::new(),
        }
    }

    pub fn with_timestamp(mut self, ts: DateTime<Local>) -> Self {
        self.timestamp = Some(ts);
        self
    }

    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = Some(level.into());
        self
    }

    pub fn with_module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn add_function_name(&mut self, name: impl Into<String>) {
        self.function_names.push(name.into());
    }

    pub fn add_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), value.into());
    }
}

fn count_indent(line: &str) -> usize {
    let mut indent = 0;
    for ch in line.chars() {
        match ch {
            ' ' => indent += 1,
            '\t' => indent += 4,
            _ => break,
        }
    }
    indent
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanExtractor {
    pub pattern: String,
    pub operation_name_group: String,
    pub kind: SpanExtractorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanExtractorKind {
    Entry,
    Exit,
    Internal,
}

pub fn parse_timestamp(timestamp_str: &str, format: Option<&str>) -> Option<DateTime<Local>> {
    let formats = if let Some(fmt) = format {
        vec![fmt.to_string()]
    } else {
        vec![
            "%Y-%m-%d %H:%M:%S".to_string(),
            "%Y-%m-%d %H:%M:%S%.3f".to_string(),
            "%Y-%m-%dT%H:%M:%S".to_string(),
            "%Y-%m-%dT%H:%M:%S%.3f".to_string(),
            "%Y/%m/%d %H:%M:%S".to_string(),
            "%d/%b/%Y:%H:%M:%S".to_string(),
            "%b %d %H:%M:%S".to_string(),
        ]
    };

    for fmt in &formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(timestamp_str, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(
                naive,
                *Local::now().offset(),
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_indent() {
        assert_eq!(count_indent("no indent"), 0);
        assert_eq!(count_indent("  two spaces"), 2);
        assert_eq!(count_indent("    four spaces"), 4);
        assert_eq!(count_indent("\tone tab"), 4);
        assert_eq!(count_indent("  \ttwo spaces and tab"), 6);
    }

    #[test]
    fn test_log_entry() {
        let entry = LogEntry::new(1, "  >>> process_data()");
        assert_eq!(entry.line_number, 1);
        assert_eq!(entry.indent_level, 2);
        assert_eq!(entry.message, ">>> process_data()");
    }

    #[test]
    fn test_parse_timestamp() {
        let ts1 = parse_timestamp("2024-01-15 10:30:45", None);
        assert!(ts1.is_some());

        let ts2 = parse_timestamp("2024-01-15T10:30:45.123", None);
        assert!(ts2.is_some());

        let ts3 = parse_timestamp("invalid", None);
        assert!(ts3.is_none());
    }
}
