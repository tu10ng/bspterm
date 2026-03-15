use std::sync::LazyLock;

use regex::Regex;

use crate::highlight_rule::{
    HighlightProtocol, HighlightRule, SemanticToken, TerminalTokenModifiers, TerminalTokenType,
};

static HEX_GROUP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b0[xX]([0-9a-fA-F]{5,})\b").expect("hex group regex should compile")
});

const HEX_GROUP_COLORS: &[&str] = &[
    "#b5cea8", // group 0 (rightmost) — light green
    "#4ec9b0", // group 1 — teal
    "#ce9178", // group 2 — salmon
    "#dcdcaa", // group 3 — light yellow
];
const HEX_PREFIX_COLOR: &str = "#858585";

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

/// Compute hex group coloring tokens for a single line of terminal output.
///
/// Groups hex digits by 4 from right-to-left (like comma-separated thousands),
/// each group colored differently. Only matches `0x`/`0X` prefixed numbers with
/// 5 or more hex digits.
pub fn compute_hex_group_tokens(line_number: usize, line_content: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();

    for mat in HEX_GROUP_REGEX.find_iter(line_content) {
        let prefix_byte_start = mat.start();
        let prefix_char_start = line_content[..prefix_byte_start].chars().count();

        // "0x" prefix token (2 chars)
        tokens.push(
            SemanticToken::new(line_number, prefix_char_start, 2, TerminalTokenType::Number)
                .with_foreground_color(Some(HEX_PREFIX_COLOR.to_string())),
        );

        // Hex digits start after "0x" (2 chars)
        let digits_byte_start = prefix_byte_start + 2; // "0x" is always 2 ASCII bytes
        let digits_str = &line_content[digits_byte_start..mat.end()];
        let digit_count = digits_str.len(); // all hex digits are ASCII, len == char count
        let digits_char_start = prefix_char_start + 2;

        // Split digits into groups of 4 from right to left
        // e.g., "1A2B3C4D5E6F" (12 digits) -> ["1A2B", "3C4D", "5E6F"]
        // e.g., "1234567" (7 digits) -> ["123", "4567"]
        let remainder = digit_count % 4;
        let mut pos = 0;
        let mut group_index_from_left = 0;

        // Total number of groups
        let total_groups = digit_count.div_ceil(4);

        // First group may be partial (< 4 digits)
        if remainder > 0 {
            let color_index = (total_groups - 1 - group_index_from_left) % HEX_GROUP_COLORS.len();
            tokens.push(
                SemanticToken::new(
                    line_number,
                    digits_char_start + pos,
                    remainder,
                    TerminalTokenType::Number,
                )
                .with_foreground_color(Some(HEX_GROUP_COLORS[color_index].to_string())),
            );
            pos += remainder;
            group_index_from_left += 1;
        }

        // Full 4-digit groups
        while pos < digit_count {
            let color_index = (total_groups - 1 - group_index_from_left) % HEX_GROUP_COLORS.len();
            tokens.push(
                SemanticToken::new(
                    line_number,
                    digits_char_start + pos,
                    4,
                    TerminalTokenType::Number,
                )
                .with_foreground_color(Some(HEX_GROUP_COLORS[color_index].to_string())),
            );
            pos += 4;
            group_index_from_left += 1;
        }
    }

    tokens
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

    #[test]
    fn test_hex_group_12_digits() {
        // 12 digits = 3 full groups of 4
        let tokens = compute_hex_group_tokens(0, "addr 0x1A2B3C4D5E6F end");

        // prefix + 3 groups = 4 tokens
        assert_eq!(tokens.len(), 4);

        // "addr " = 5 chars, prefix at col 5
        assert_eq!(tokens[0].start_col, 5);
        assert_eq!(tokens[0].length, 2); // "0x"
        assert_eq!(
            tokens[0].foreground_color.as_deref(),
            Some(HEX_PREFIX_COLOR)
        );

        // Group from left: "1A2B" at col 7, color index = (3-1-0)%4 = 2 => salmon
        assert_eq!(tokens[1].start_col, 7);
        assert_eq!(tokens[1].length, 4);
        assert_eq!(
            tokens[1].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[2])
        );

        // "3C4D" at col 11, color index = (3-1-1)%4 = 1 => teal
        assert_eq!(tokens[2].start_col, 11);
        assert_eq!(tokens[2].length, 4);
        assert_eq!(
            tokens[2].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[1])
        );

        // "5E6F" at col 15, color index = (3-1-2)%4 = 0 => light green
        assert_eq!(tokens[3].start_col, 15);
        assert_eq!(tokens[3].length, 4);
        assert_eq!(
            tokens[3].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[0])
        );
    }

    #[test]
    fn test_hex_group_odd_digit_count() {
        // 7 digits -> partial first group (3) + full group (4)
        let tokens = compute_hex_group_tokens(0, "0x1234567");

        assert_eq!(tokens.len(), 3); // prefix + 2 groups

        // prefix
        assert_eq!(tokens[0].start_col, 0);
        assert_eq!(tokens[0].length, 2);

        // "123" (partial, 3 chars) at col 2, color index = (2-1-0)%4 = 1 => teal
        assert_eq!(tokens[1].start_col, 2);
        assert_eq!(tokens[1].length, 3);
        assert_eq!(
            tokens[1].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[1])
        );

        // "4567" at col 5, color index = (2-1-1)%4 = 0 => light green
        assert_eq!(tokens[2].start_col, 5);
        assert_eq!(tokens[2].length, 4);
        assert_eq!(
            tokens[2].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[0])
        );
    }

    #[test]
    fn test_hex_group_short_hex_no_tokens() {
        // < 5 digits should not match
        let tokens = compute_hex_group_tokens(0, "0xFF");
        assert!(tokens.is_empty());

        let tokens = compute_hex_group_tokens(0, "0x1234");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_hex_group_exactly_5_digits() {
        // 5 digits = partial (1) + full (4) = 2 groups
        let tokens = compute_hex_group_tokens(0, "0x12345");

        assert_eq!(tokens.len(), 3); // prefix + 2 groups

        // partial "1" at col 2, color index = (2-1-0)%4 = 1 => teal
        assert_eq!(tokens[1].start_col, 2);
        assert_eq!(tokens[1].length, 1);

        // "2345" at col 3, color index = (2-1-1)%4 = 0 => light green
        assert_eq!(tokens[2].start_col, 3);
        assert_eq!(tokens[2].length, 4);
    }

    #[test]
    fn test_hex_group_multiple_on_same_line() {
        let tokens = compute_hex_group_tokens(0, "a=0xDEADBEEF b=0x1234567890");

        // First: 0xDEADBEEF (8 digits) -> prefix + 2 groups = 3
        // Second: 0x1234567890 (10 digits) -> prefix + 1 partial + 2 full = 4
        assert_eq!(tokens.len(), 7);

        // First number prefix at col 2
        assert_eq!(tokens[0].start_col, 2);
        assert_eq!(tokens[0].length, 2);
    }

    #[test]
    fn test_hex_group_uppercase_prefix() {
        let tokens = compute_hex_group_tokens(0, "0XABCDE");
        assert_eq!(tokens.len(), 3); // prefix + 2 groups
        assert_eq!(tokens[0].length, 2); // "0X"
        assert_eq!(
            tokens[0].foreground_color.as_deref(),
            Some(HEX_PREFIX_COLOR)
        );
    }

    #[test]
    fn test_hex_group_utf8_before_hex() {
        // Chinese text before hex number
        let tokens = compute_hex_group_tokens(0, "地址：0xABCDEF");

        assert_eq!(tokens.len(), 3); // prefix + 2 groups

        // "地址：" = 3 chars
        assert_eq!(tokens[0].start_col, 3);
        assert_eq!(tokens[0].length, 2); // "0x"

        // "AB" at col 5 (partial, 2 chars)
        assert_eq!(tokens[1].start_col, 5);
        assert_eq!(tokens[1].length, 2);

        // "CDEF" at col 7 (full group)
        assert_eq!(tokens[2].start_col, 7);
        assert_eq!(tokens[2].length, 4);
    }

    #[test]
    fn test_hex_group_8_digits() {
        // 8 digits = 2 full groups, no partial
        let tokens = compute_hex_group_tokens(0, "0xDEADBEEF");

        assert_eq!(tokens.len(), 3); // prefix + 2 groups

        // "DEAD" at col 2, color index = (2-1-0)%4 = 1 => teal
        assert_eq!(tokens[1].start_col, 2);
        assert_eq!(tokens[1].length, 4);
        assert_eq!(
            tokens[1].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[1])
        );

        // "BEEF" at col 6, color index = (2-1-1)%4 = 0 => light green
        assert_eq!(tokens[2].start_col, 6);
        assert_eq!(tokens[2].length, 4);
        assert_eq!(
            tokens[2].foreground_color.as_deref(),
            Some(HEX_GROUP_COLORS[0])
        );
    }
}
