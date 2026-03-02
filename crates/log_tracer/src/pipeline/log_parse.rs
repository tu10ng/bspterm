use anyhow::Result;
use rayon::prelude::*;

use super::{AnalysisContext, AnalysisStep};
use crate::parser::KeywordMatcher;
use crate::AnalysisProgress;

pub struct LogParseStep {
    use_parallel: bool,
}

impl LogParseStep {
    pub fn new() -> Self {
        Self { use_parallel: true }
    }

    pub fn sequential() -> Self {
        Self {
            use_parallel: false,
        }
    }

    fn should_include_entry(
        &self,
        entry: &crate::parser::LogEntry,
        ctx: &AnalysisContext,
    ) -> bool {
        if let Some(ref level) = entry.level {
            if ctx.filter_config.exclude_levels.contains(level) {
                return false;
            }
        }

        for keyword in &ctx.filter_config.exclude_keywords {
            if entry.message.contains(keyword) {
                return false;
            }
        }

        if let Some(ref modules) = ctx.filter_config.include_modules {
            if let Some(ref module) = entry.module {
                return modules.contains(module);
            }
            return false;
        }

        true
    }
}

impl Default for LogParseStep {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisStep for LogParseStep {
    fn name(&self) -> &str {
        "log_parse"
    }

    fn process(&self, ctx: &mut AnalysisContext) -> Result<()> {
        let total_lines = ctx.log_content.lines().count();
        log::info!("[LogParseStep] Starting to parse {} lines", total_lines);

        ctx.progress = AnalysisProgress::Parsing {
            current: 0,
            total: total_lines,
        };

        let lines: Vec<(usize, &str)> = ctx.log_content.lines().enumerate().collect();

        let entries: Vec<_> = if self.use_parallel && lines.len() > 100 {
            if let Some(ref rule) = ctx.rule {
                lines
                    .par_iter()
                    .filter_map(|(line_num, line)| {
                        rule.parse_line(*line_num + 1, line)
                    })
                    .collect()
            } else {
                lines
                    .par_iter()
                    .map(|(line_num, line)| {
                        let mut entry = crate::parser::LogEntry::new(*line_num + 1, *line);
                        let matcher = KeywordMatcher::new(vec![]);
                        for func in matcher.extract_function_names(line) {
                            entry.add_function_name(func);
                        }
                        entry
                    })
                    .collect()
            }
        } else if let Some(ref rule) = ctx.rule {
            lines
                .iter()
                .filter_map(|(line_num, line)| {
                    rule.parse_line(*line_num + 1, line)
                })
                .collect()
        } else {
            lines
                .iter()
                .map(|(line_num, line)| {
                    let mut entry = crate::parser::LogEntry::new(*line_num + 1, *line);
                    let matcher = KeywordMatcher::new(vec![]);
                    for func in matcher.extract_function_names(line) {
                        entry.add_function_name(func);
                    }
                    entry
                })
                .collect()
        };

        let before_filter = entries.len();
        let filtered: Vec<_> = entries
            .into_iter()
            .filter(|e| self.should_include_entry(e, ctx))
            .collect();

        log::info!(
            "[LogParseStep] Parsed {} entries, filtered to {} (removed {})",
            before_filter,
            filtered.len(),
            before_filter - filtered.len()
        );

        let function_count: usize = filtered.iter().map(|e| e.function_names.len()).sum();
        log::info!(
            "[LogParseStep] Extracted {} total function references",
            function_count
        );

        ctx.entries = filtered;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogFilterConfig;

    #[test]
    fn test_log_parse_step() {
        let content = r#"
2024-01-15 10:30:45 [network] calling handle_request()
2024-01-15 10:30:46 [parser] process_data() started
>>> validate_input
<<< validate_input
"#;

        let mut ctx = AnalysisContext::new(content.to_string());
        let step = LogParseStep::sequential();
        step.process(&mut ctx).unwrap();

        assert!(!ctx.entries.is_empty());
        let all_funcs: Vec<_> = ctx
            .entries
            .iter()
            .flat_map(|e| e.function_names.clone())
            .collect();
        assert!(
            all_funcs.contains(&"handle_request".to_string())
                || all_funcs.contains(&"validate_input".to_string())
        );
    }

    #[test]
    fn test_filter_by_level() {
        let content = "DEBUG ignore this\nINFO keep this";

        let mut ctx = AnalysisContext::new(content.to_string());
        ctx.filter_config = LogFilterConfig {
            exclude_levels: vec!["DEBUG".to_string()],
            exclude_keywords: vec![],
            include_modules: None,
        };

        for (i, line) in content.lines().enumerate() {
            let mut entry = crate::parser::LogEntry::new(i + 1, line);
            if line.starts_with("DEBUG") {
                entry.level = Some("DEBUG".to_string());
            } else {
                entry.level = Some("INFO".to_string());
            }
            ctx.entries.push(entry);
        }

        let step = LogParseStep::new();
        let filtered: Vec<_> = ctx
            .entries
            .iter()
            .filter(|e| step.should_include_entry(e, &ctx))
            .collect();

        assert_eq!(filtered.len(), 1);
    }
}
