use std::path::PathBuf;

use anyhow::{Context as _, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{LogEntry, SpanExtractor, SpanExtractorKind, parse_timestamp};
use crate::ContextInferenceConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogParseRule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub line_pattern: String,
    pub timestamp_format: Option<String>,
    pub keyword_patterns: Vec<KeywordPattern>,
    pub context_inference: ContextInferenceConfig,
    pub span_extractors: Vec<SpanExtractor>,
}

impl LogParseRule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            line_pattern: r"^(?P<message>.*)$".to_string(),
            timestamp_format: None,
            keyword_patterns: Vec::new(),
            context_inference: ContextInferenceConfig::default(),
            span_extractors: Vec::new(),
        }
    }

    pub fn with_line_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.line_pattern = pattern.into();
        self
    }

    pub fn with_timestamp_format(mut self, format: impl Into<String>) -> Self {
        self.timestamp_format = Some(format.into());
        self
    }

    pub fn add_keyword_pattern(mut self, pattern: KeywordPattern) -> Self {
        self.keyword_patterns.push(pattern);
        self
    }

    pub fn add_span_extractor(mut self, extractor: SpanExtractor) -> Self {
        self.span_extractors.push(extractor);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordPattern {
    pub pattern: String,
    pub captures: Vec<String>,
}

impl KeywordPattern {
    pub fn new(pattern: impl Into<String>, captures: Vec<String>) -> Self {
        Self {
            pattern: pattern.into(),
            captures,
        }
    }
}

pub struct CompiledLogRule {
    pub rule: LogParseRule,
    line_regex: Regex,
    keyword_regexes: Vec<(Regex, Vec<String>)>,
    span_extractor_regexes: Vec<(Regex, SpanExtractorKind, String)>,
}

impl CompiledLogRule {
    pub fn compile(rule: LogParseRule) -> Result<Self> {
        let line_regex =
            Regex::new(&rule.line_pattern).context("Failed to compile line pattern")?;

        let mut keyword_regexes = Vec::new();
        for kp in &rule.keyword_patterns {
            let re = Regex::new(&kp.pattern).context("Failed to compile keyword pattern")?;
            keyword_regexes.push((re, kp.captures.clone()));
        }

        let mut span_extractor_regexes = Vec::new();
        for se in &rule.span_extractors {
            let re = Regex::new(&se.pattern).context("Failed to compile span extractor pattern")?;
            span_extractor_regexes.push((re, se.kind, se.operation_name_group.clone()));
        }

        Ok(Self {
            rule,
            line_regex,
            keyword_regexes,
            span_extractor_regexes,
        })
    }

    pub fn parse_line(&self, line_number: usize, line: &str) -> Option<LogEntry> {
        let caps = self.line_regex.captures(line)?;

        let mut entry = LogEntry::new(line_number, line);

        if let Some(ts_match) = caps.name("timestamp") {
            if let Some(ts) =
                parse_timestamp(ts_match.as_str(), self.rule.timestamp_format.as_deref())
            {
                entry.timestamp = Some(ts);
            }
        }

        if let Some(level_match) = caps.name("level") {
            entry.level = Some(level_match.as_str().to_uppercase());
        }

        if let Some(module_match) = caps.name("module") {
            entry.module = Some(module_match.as_str().to_string());
        }

        if let Some(message_match) = caps.name("message") {
            entry.message = message_match.as_str().to_string();
        }

        let message_clone = entry.message.clone();
        for (regex, capture_names) in &self.keyword_regexes {
            for caps in regex.captures_iter(&message_clone) {
                for name in capture_names {
                    if let Some(mat) = caps.name(name) {
                        let value = mat.as_str().to_string();
                        if name == "function" {
                            entry.add_function_name(&value);
                        }
                        entry.add_attribute(name.clone(), value);
                    }
                }
            }
        }

        Some(entry)
    }

    pub fn extract_span_info(
        &self,
        message: &str,
    ) -> Option<(String, SpanExtractorKind)> {
        for (regex, kind, group_name) in &self.span_extractor_regexes {
            if let Some(caps) = regex.captures(message) {
                if let Some(mat) = caps.name(group_name) {
                    return Some((mat.as_str().to_string(), *kind));
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogParseRuleStore {
    rules: Vec<LogParseRule>,
    version: u32,
}

impl Default for LogParseRuleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParseRuleStore {
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
            version: 1,
        }
    }

    pub fn rules(&self) -> &[LogParseRule] {
        &self.rules
    }

    pub fn get_rule(&self, id: Uuid) -> Option<&LogParseRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn add_rule(&mut self, rule: LogParseRule) {
        self.rules.push(rule);
    }

    pub fn update_rule(&mut self, rule: LogParseRule) {
        if let Some(pos) = self.rules.iter().position(|r| r.id == rule.id) {
            self.rules[pos] = rule;
        }
    }

    pub fn remove_rule(&mut self, id: Uuid) {
        self.rules.retain(|r| r.id != id);
    }

    pub fn default_rules() -> Vec<LogParseRule> {
        vec![
            LogParseRule::new("Module Timestamp")
                .with_line_pattern(
                    r"^(?P<timestamp>[\d-]+ [\d:]+)\s+\[(?P<module>\w+)\]\s+(?P<message>.*)$",
                )
                .add_keyword_pattern(KeywordPattern::new(
                    r"(?:calling|enter|invoke)\s+(?P<function>\w+)",
                    vec!["function".to_string()],
                ))
                .add_keyword_pattern(KeywordPattern::new(
                    r"(?P<function>\w+)\s*\(\s*\)",
                    vec!["function".to_string()],
                )),
            LogParseRule::new("Lua Trace")
                .with_line_pattern(
                    r"^\[TRACE\]\s+(?P<file>[\w./]+):(?P<line>\d+)\s+(?P<message>.*)$",
                )
                .add_keyword_pattern(KeywordPattern::new(
                    r"(?P<function>\w+)\s*\(",
                    vec!["function".to_string()],
                )),
            LogParseRule::new("Printf Debug")
                .with_line_pattern(r"^(?P<message>.*)$")
                .add_keyword_pattern(KeywordPattern::new(
                    r">>>\s*(?P<function>\w+)",
                    vec!["function".to_string()],
                ))
                .add_keyword_pattern(KeywordPattern::new(
                    r"<<<\s*(?P<function>\w+)",
                    vec!["function".to_string()],
                )),
            LogParseRule::new("Standard Log")
                .with_line_pattern(
                    r"^(?P<timestamp>[\d-]+[T ][\d:.]+)\s+(?P<level>\w+)\s+(?P<message>.*)$",
                )
                .add_keyword_pattern(KeywordPattern::new(
                    r"(?P<function>\w+)(?:\(\)|:)",
                    vec!["function".to_string()],
                )),
        ]
    }

    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bspterm");
        config_dir.join("log_parse_rules.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let store: Self = serde_json::from_str(&content)?;
            log::info!(
                "[LogParseRuleStore] Loaded {} rules from {:?}",
                store.rules.len(),
                path
            );
            Ok(store)
        } else {
            log::info!(
                "[LogParseRuleStore] No config found at {:?}, using defaults",
                path
            );
            Ok(Self::new())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        log::info!(
            "[LogParseRuleStore] Saved {} rules to {:?}",
            self.rules.len(),
            path
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_rule() {
        let rule = LogParseRule::new("Test")
            .with_line_pattern(r"^(?P<timestamp>[\d-]+ [\d:]+)\s+\[(?P<module>\w+)\]\s+(?P<message>.*)$")
            .add_keyword_pattern(KeywordPattern::new(
                r"calling\s+(?P<function>\w+)",
                vec!["function".to_string()],
            ));

        let compiled = CompiledLogRule::compile(rule).unwrap();

        let entry = compiled
            .parse_line(1, "2024-01-15 10:30:45 [network] calling handle_request()")
            .unwrap();

        assert_eq!(entry.module, Some("network".to_string()));
        assert!(entry.function_names.contains(&"handle_request".to_string()));
    }

    #[test]
    fn test_printf_debug_rule() {
        let rule = LogParseRule::new("Printf Debug")
            .with_line_pattern(r"^(?P<message>.*)$")
            .add_keyword_pattern(KeywordPattern::new(
                r">>>\s*(?P<function>\w+)",
                vec!["function".to_string()],
            ));

        let compiled = CompiledLogRule::compile(rule).unwrap();

        let entry = compiled.parse_line(1, ">>> process_data").unwrap();
        assert!(entry.function_names.contains(&"process_data".to_string()));
    }
}
