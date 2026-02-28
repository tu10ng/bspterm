use regex::Regex;

use crate::highlight_rule::{
    HighlightProtocol, HighlightRule, SemanticToken, TerminalTokenModifiers, TerminalTokenType,
};

/// A compiled highlight rule ready for matching.
pub struct CompiledRule {
    pub rule: HighlightRule,
    pub regex: Regex,
}

impl CompiledRule {
    /// Compile a highlight rule into a regex-ready form.
    pub fn new(rule: HighlightRule) -> Option<Self> {
        let pattern = if rule.case_insensitive {
            format!("(?i){}", rule.pattern)
        } else {
            rule.pattern.clone()
        };

        match Regex::new(&pattern) {
            Ok(regex) => Some(Self { rule, regex }),
            Err(err) => {
                log::warn!(
                    "Failed to compile highlight rule '{}' pattern '{}': {}",
                    rule.name,
                    rule.pattern,
                    err
                );
                None
            }
        }
    }

    /// Check if this rule matches the given protocol.
    pub fn matches_protocol(&self, protocol: Option<&HighlightProtocol>) -> bool {
        self.rule.protocol.matches(protocol)
    }
}

/// The terminal language server that provides semantic highlighting.
pub struct TerminalLanguageServer {
    /// Compiled highlight rules sorted by priority.
    compiled_rules: Vec<CompiledRule>,
}

impl TerminalLanguageServer {
    /// Create a new terminal language server with the given rules.
    pub fn new(rules: &[HighlightRule]) -> Self {
        let mut compiled_rules: Vec<CompiledRule> = rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| CompiledRule::new(r.clone()))
            .collect();

        // Sort by priority (higher first)
        compiled_rules.sort_by(|a, b| b.rule.priority.cmp(&a.rule.priority));

        Self { compiled_rules }
    }

    /// Update the rules. This recompiles all patterns.
    pub fn update_rules(&mut self, rules: &[HighlightRule]) {
        let mut compiled_rules: Vec<CompiledRule> = rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| CompiledRule::new(r.clone()))
            .collect();

        compiled_rules.sort_by(|a, b| b.rule.priority.cmp(&a.rule.priority));
        self.compiled_rules = compiled_rules;
    }

    /// Compute semantic tokens for a single line of terminal output.
    ///
    /// # Arguments
    /// * `line_number` - The line number (0-indexed)
    /// * `line_content` - The text content of the line
    /// * `protocol` - The current terminal protocol filter
    ///
    /// # Returns
    /// A vector of semantic tokens for this line, sorted by start position.
    pub fn compute_line_tokens(
        &self,
        line_number: usize,
        line_content: &str,
        protocol: Option<&HighlightProtocol>,
    ) -> Vec<SemanticToken> {
        let mut tokens = Vec::new();

        for compiled_rule in &self.compiled_rules {
            if !compiled_rule.matches_protocol(protocol) {
                continue;
            }

            for mat in compiled_rule.regex.find_iter(line_content) {
                // Convert byte positions to character positions for proper UTF-8 support
                let start_col = line_content[..mat.start()].chars().count();
                let length = line_content[mat.start()..mat.end()].chars().count();

                // Check if this region overlaps with any existing token
                // Since rules are sorted by priority, we skip if there's already a token
                let overlaps = tokens.iter().any(|t: &SemanticToken| {
                    let t_end = t.start_col + t.length;
                    let mat_end = start_col + length;
                    // Check for any overlap
                    !(mat_end <= t.start_col || start_col >= t_end)
                });

                if !overlaps {
                    tokens.push(
                        SemanticToken::new(
                            line_number,
                            start_col,
                            length,
                            compiled_rule.rule.token_type.clone(),
                        )
                        .with_modifiers(compiled_rule.rule.modifiers)
                        .with_foreground_color(compiled_rule.rule.foreground_color.clone())
                        .with_background_color(compiled_rule.rule.background_color.clone()),
                    );
                }
            }
        }

        // Sort tokens by start position for consistent rendering
        tokens.sort_by_key(|t| t.start_col);
        tokens
    }

    /// Compute semantic tokens for a range of lines.
    ///
    /// # Arguments
    /// * `lines` - Iterator of (line_number, line_content) pairs
    /// * `protocol` - The current terminal protocol filter
    ///
    /// # Returns
    /// A vector of semantic tokens for all lines.
    pub fn compute_tokens<'a>(
        &self,
        lines: impl Iterator<Item = (usize, &'a str)>,
        protocol: Option<&HighlightProtocol>,
    ) -> Vec<SemanticToken> {
        let mut all_tokens = Vec::new();

        for (line_number, line_content) in lines {
            let line_tokens = self.compute_line_tokens(line_number, line_content, protocol);
            all_tokens.extend(line_tokens);
        }

        all_tokens
    }

    /// Get tokens that apply to a specific position.
    ///
    /// # Arguments
    /// * `tokens` - The computed tokens for a document
    /// * `line` - The line number
    /// * `col` - The column number
    ///
    /// # Returns
    /// The token at the given position, if any.
    pub fn token_at_position(tokens: &[SemanticToken], line: usize, col: usize) -> Option<&SemanticToken> {
        tokens.iter().find(|t| t.contains(line, col))
    }

    /// Get all tokens on a specific line.
    pub fn tokens_on_line(tokens: &[SemanticToken], line: usize) -> Vec<&SemanticToken> {
        tokens.iter().filter(|t| t.line == line).collect()
    }

    /// Get the number of compiled rules.
    pub fn rule_count(&self) -> usize {
        self.compiled_rules.len()
    }
}

/// Terminal document for tracking content and cached tokens.
pub struct TerminalDocument {
    /// Document version for cache invalidation.
    pub version: u32,
    /// Cached semantic tokens.
    cached_tokens: Vec<SemanticToken>,
    /// Protocol filter for this terminal.
    pub protocol: Option<HighlightProtocol>,
}

impl TerminalDocument {
    /// Create a new terminal document.
    pub fn new(protocol: Option<HighlightProtocol>) -> Self {
        Self {
            version: 0,
            cached_tokens: Vec::new(),
            protocol,
        }
    }

    /// Update the cached tokens.
    pub fn update_tokens(&mut self, tokens: Vec<SemanticToken>) {
        self.version += 1;
        self.cached_tokens = tokens;
    }

    /// Clear the cached tokens.
    pub fn clear_tokens(&mut self) {
        self.version += 1;
        self.cached_tokens.clear();
    }

    /// Get the cached tokens.
    pub fn tokens(&self) -> &[SemanticToken] {
        &self.cached_tokens
    }

    /// Get tokens for a specific line from the cache.
    pub fn tokens_for_line(&self, line: usize) -> Vec<&SemanticToken> {
        self.cached_tokens.iter().filter(|t| t.line == line).collect()
    }
}

/// Get the default color for a token type.
/// Returns a hex color string.
pub fn default_token_color(token_type: &TerminalTokenType) -> &'static str {
    match token_type {
        TerminalTokenType::Error => "#f44747",      // Bright Red
        TerminalTokenType::Warning => "#ffcc00",    // Bright Yellow
        TerminalTokenType::Info => "#3794ff",       // Bright Blue
        TerminalTokenType::Debug => "#b267e6",      // Magenta
        TerminalTokenType::Timestamp => "#858585",  // Medium Gray
        TerminalTokenType::IpAddress => "#ce9178",  // Salmon Orange
        TerminalTokenType::Url => "#4ec9b0",        // Teal Cyan
        TerminalTokenType::Path => "#dcdcaa",       // Yellow
        TerminalTokenType::Number => "#b5cea8",     // Light Green
        TerminalTokenType::Success => "#89d185",    // Light Green
        TerminalTokenType::Custom(_) => "#d4d4d4",  // Light Gray
    }
}

/// Get the default modifiers for a token type.
pub fn default_token_modifiers(token_type: &TerminalTokenType) -> TerminalTokenModifiers {
    match token_type {
        TerminalTokenType::Error => TerminalTokenModifiers::new().with_bold(),
        TerminalTokenType::Warning => TerminalTokenModifiers::new(),
        TerminalTokenType::Url => TerminalTokenModifiers::new().with_underline(),
        _ => TerminalTokenModifiers::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rules() -> Vec<HighlightRule> {
        vec![
            HighlightRule::new("Error", r"\bERROR\b", TerminalTokenType::Error)
                .with_case_insensitive(true)
                .with_priority(100),
            HighlightRule::new("Warning", r"\bWARN(ING)?\b", TerminalTokenType::Warning)
                .with_case_insensitive(true)
                .with_priority(90),
            HighlightRule::new("IP", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b", TerminalTokenType::IpAddress)
                .with_priority(60),
        ]
    }

    #[test]
    fn test_compute_line_tokens() {
        let server = TerminalLanguageServer::new(&test_rules());

        let tokens = server.compute_line_tokens(0, "ERROR: Connection failed to 192.168.1.1", None);

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TerminalTokenType::Error);
        assert_eq!(tokens[0].start_col, 0);
        assert_eq!(tokens[0].length, 5);

        assert_eq!(tokens[1].token_type, TerminalTokenType::IpAddress);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let server = TerminalLanguageServer::new(&test_rules());

        let tokens1 = server.compute_line_tokens(0, "error: something", None);
        let tokens2 = server.compute_line_tokens(0, "ERROR: something", None);
        let tokens3 = server.compute_line_tokens(0, "Error: something", None);

        assert_eq!(tokens1.len(), 1);
        assert_eq!(tokens2.len(), 1);
        assert_eq!(tokens3.len(), 1);
    }

    #[test]
    fn test_priority_ordering() {
        let rules = vec![
            HighlightRule::new("Low", r"\btest\b", TerminalTokenType::Debug)
                .with_priority(10),
            HighlightRule::new("High", r"\btest\b", TerminalTokenType::Error)
                .with_priority(100),
        ];

        let server = TerminalLanguageServer::new(&rules);
        let tokens = server.compute_line_tokens(0, "test message", None);

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TerminalTokenType::Error);
    }

    #[test]
    fn test_no_overlapping_tokens() {
        let rules = vec![
            HighlightRule::new("Full", r"test error", TerminalTokenType::Error)
                .with_priority(100),
            HighlightRule::new("Partial", r"error", TerminalTokenType::Warning)
                .with_priority(50),
        ];

        let server = TerminalLanguageServer::new(&rules);
        let tokens = server.compute_line_tokens(0, "test error occurred", None);

        // Should only have one token since "error" overlaps with "test error"
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TerminalTokenType::Error);
    }

    #[test]
    fn test_protocol_filtering() {
        let rules = vec![
            HighlightRule::new("SSH Only", r"\bSSH\b", TerminalTokenType::Info)
                .with_protocol(HighlightProtocol::Ssh),
            HighlightRule::new("All", r"\bALL\b", TerminalTokenType::Info)
                .with_protocol(HighlightProtocol::All),
        ];

        let server = TerminalLanguageServer::new(&rules);

        // With SSH protocol
        let tokens = server.compute_line_tokens(0, "SSH ALL test", Some(&HighlightProtocol::Ssh));
        assert_eq!(tokens.len(), 2);

        // With Telnet protocol
        let tokens = server.compute_line_tokens(0, "SSH ALL test", Some(&HighlightProtocol::Telnet));
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].start_col, 4); // "ALL" position
    }

    #[test]
    fn test_multiple_matches_same_line() {
        let server = TerminalLanguageServer::new(&test_rules());

        let tokens = server.compute_line_tokens(
            0,
            "ERROR at 192.168.1.1: WARNING from 10.0.0.1",
            None,
        );

        assert_eq!(tokens.len(), 4);
    }

    #[test]
    fn test_invalid_regex_handling() {
        let rules = vec![
            HighlightRule::new("Invalid", r"[invalid", TerminalTokenType::Error),
            HighlightRule::new("Valid", r"\bERROR\b", TerminalTokenType::Error),
        ];

        let server = TerminalLanguageServer::new(&rules);
        // Should only have the valid rule compiled
        assert_eq!(server.rule_count(), 1);
    }

    #[test]
    fn test_token_at_position() {
        let server = TerminalLanguageServer::new(&test_rules());
        let tokens = server.compute_line_tokens(0, "ERROR: test", None);

        assert!(TerminalLanguageServer::token_at_position(&tokens, 0, 0).is_some());
        assert!(TerminalLanguageServer::token_at_position(&tokens, 0, 4).is_some());
        assert!(TerminalLanguageServer::token_at_position(&tokens, 0, 5).is_none());
        assert!(TerminalLanguageServer::token_at_position(&tokens, 1, 0).is_none());
    }

    #[test]
    fn test_utf8_character_positions() {
        let rules = vec![
            HighlightRule::new(
                "Timestamp",
                r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}",
                TerminalTokenType::Timestamp,
            )
            .with_priority(50),
        ];

        let server = TerminalLanguageServer::new(&rules);

        // Test with Chinese characters before the timestamp
        // "登录时间: " has 5 characters but more bytes in UTF-8
        let tokens = server.compute_line_tokens(0, "登录时间: 2026-02-28 09:46:21.", None);

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TerminalTokenType::Timestamp);
        // Position should be in characters, not bytes
        // "登录时间: " = 5 chars + 1 space = 6 characters before timestamp
        assert_eq!(tokens[0].start_col, 6);
        // Timestamp "2026-02-28 09:46:21" = 19 characters
        assert_eq!(tokens[0].length, 19);
    }

    #[test]
    fn test_timestamp_position_in_english_text() {
        let rules = vec![
            HighlightRule::new(
                "Timestamp",
                r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}",
                TerminalTokenType::Timestamp,
            )
            .with_priority(50),
        ];

        let server = TerminalLanguageServer::new(&rules);

        // Test the exact scenario from the bug report
        let tokens = server.compute_line_tokens(
            0,
            "The current login time is 2026-02-28 09:46:21.",
            None,
        );

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TerminalTokenType::Timestamp);
        // "The current login time is " = 26 characters
        assert_eq!(tokens[0].start_col, 26);
        // "2026-02-28 09:46:21" = 19 characters
        assert_eq!(tokens[0].length, 19);
    }
}
