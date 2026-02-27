use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

/// Semantic token types for terminal output highlighting.
/// These correspond to LSP semantic token types but are specialized for terminal output.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTokenType {
    /// Error keywords (error, fail, fatal, critical, exception)
    #[default]
    Error,
    /// Warning keywords (warn, warning, caution, deprecated)
    Warning,
    /// Info keywords
    Info,
    /// Debug keywords
    Debug,
    /// Timestamp patterns
    Timestamp,
    /// IP address patterns
    IpAddress,
    /// URL patterns
    Url,
    /// File path patterns
    Path,
    /// Numeric values
    Number,
    /// Success keywords (success, ok, passed)
    Success,
    /// User-defined custom type
    Custom(String),
}

impl TerminalTokenType {
    /// Get the display label for this token type.
    pub fn label(&self) -> &str {
        match self {
            TerminalTokenType::Error => "Error",
            TerminalTokenType::Warning => "Warning",
            TerminalTokenType::Info => "Info",
            TerminalTokenType::Debug => "Debug",
            TerminalTokenType::Timestamp => "Timestamp",
            TerminalTokenType::IpAddress => "IP Address",
            TerminalTokenType::Url => "URL",
            TerminalTokenType::Path => "Path",
            TerminalTokenType::Number => "Number",
            TerminalTokenType::Success => "Success",
            TerminalTokenType::Custom(name) => name.as_str(),
        }
    }

    /// Get the index in the semantic token legend.
    pub fn legend_index(&self) -> u32 {
        match self {
            TerminalTokenType::Error => 0,
            TerminalTokenType::Warning => 1,
            TerminalTokenType::Info => 2,
            TerminalTokenType::Debug => 3,
            TerminalTokenType::Timestamp => 4,
            TerminalTokenType::IpAddress => 5,
            TerminalTokenType::Url => 6,
            TerminalTokenType::Path => 7,
            TerminalTokenType::Number => 8,
            TerminalTokenType::Success => 9,
            TerminalTokenType::Custom(_) => 10,
        }
    }

    /// Get all standard token types for the semantic token legend.
    pub fn standard_types() -> Vec<&'static str> {
        vec![
            "error",
            "warning",
            "info",
            "debug",
            "timestamp",
            "ipAddress",
            "url",
            "path",
            "number",
            "success",
            "custom",
        ]
    }
}

/// Semantic token modifiers as a bitfield.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTokenModifiers(pub u32);

impl TerminalTokenModifiers {
    pub const NONE: u32 = 0;
    pub const BOLD: u32 = 1 << 0;
    pub const ITALIC: u32 = 1 << 1;
    pub const UNDERLINE: u32 = 1 << 2;
    pub const DIM: u32 = 1 << 3;
    pub const STRIKETHROUGH: u32 = 1 << 4;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn with_bold(mut self) -> Self {
        self.0 |= Self::BOLD;
        self
    }

    pub fn with_italic(mut self) -> Self {
        self.0 |= Self::ITALIC;
        self
    }

    pub fn with_underline(mut self) -> Self {
        self.0 |= Self::UNDERLINE;
        self
    }

    pub fn with_dim(mut self) -> Self {
        self.0 |= Self::DIM;
        self
    }

    pub fn with_strikethrough(mut self) -> Self {
        self.0 |= Self::STRIKETHROUGH;
        self
    }

    pub fn is_bold(&self) -> bool {
        self.0 & Self::BOLD != 0
    }

    pub fn is_italic(&self) -> bool {
        self.0 & Self::ITALIC != 0
    }

    pub fn is_underline(&self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }

    pub fn is_dim(&self) -> bool {
        self.0 & Self::DIM != 0
    }

    pub fn is_strikethrough(&self) -> bool {
        self.0 & Self::STRIKETHROUGH != 0
    }

    /// Get all standard modifier names for the semantic token legend.
    pub fn standard_modifiers() -> Vec<&'static str> {
        vec!["bold", "italic", "underline", "dim", "strikethrough"]
    }
}

/// Protocol filter for highlight rules.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightProtocol {
    /// Apply to all terminals
    #[default]
    All,
    /// Only SSH connections
    Ssh,
    /// Only Telnet connections
    Telnet,
    /// Only local PTY
    Local,
}

impl HighlightProtocol {
    pub fn label(&self) -> &'static str {
        match self {
            HighlightProtocol::All => "All",
            HighlightProtocol::Ssh => "SSH",
            HighlightProtocol::Telnet => "Telnet",
            HighlightProtocol::Local => "Local",
        }
    }

    /// Check if this protocol filter matches the given protocol.
    pub fn matches(&self, protocol: Option<&HighlightProtocol>) -> bool {
        match (self, protocol) {
            (HighlightProtocol::All, _) => true,
            (HighlightProtocol::Ssh, Some(HighlightProtocol::Ssh)) => true,
            (HighlightProtocol::Telnet, Some(HighlightProtocol::Telnet)) => true,
            (HighlightProtocol::Local, Some(HighlightProtocol::Local)) => true,
            (HighlightProtocol::Local, None) => true,
            _ => false,
        }
    }
}

/// A highlight rule definition for terminal output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HighlightRule {
    /// Unique identifier for this rule
    pub id: Uuid,
    /// Human-readable name for the rule
    pub name: String,
    /// Regular expression pattern to match
    pub pattern: String,
    /// The semantic token type to assign to matches
    pub token_type: TerminalTokenType,
    /// Style modifiers to apply
    #[serde(default)]
    pub modifiers: TerminalTokenModifiers,
    /// Whether this rule is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether the pattern is case-insensitive
    #[serde(default)]
    pub case_insensitive: bool,
    /// Protocol filter for when to apply this rule
    #[serde(default)]
    pub protocol: HighlightProtocol,
    /// Priority for rule ordering (higher = evaluated first)
    #[serde(default)]
    pub priority: i32,
    /// Optional foreground color override (hex format like "#ff0000")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_color: Option<String>,
    /// Optional background color override (hex format)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
}

impl HighlightRule {
    /// Create a new highlight rule with the given name, pattern, and token type.
    pub fn new(
        name: impl Into<String>,
        pattern: impl Into<String>,
        token_type: TerminalTokenType,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            pattern: pattern.into(),
            token_type,
            modifiers: TerminalTokenModifiers::default(),
            enabled: true,
            case_insensitive: false,
            protocol: HighlightProtocol::All,
            priority: 0,
            foreground_color: None,
            background_color: None,
        }
    }

    /// Set case insensitivity for this rule.
    pub fn with_case_insensitive(mut self, case_insensitive: bool) -> Self {
        self.case_insensitive = case_insensitive;
        self
    }

    /// Set the priority for this rule.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the protocol filter for this rule.
    pub fn with_protocol(mut self, protocol: HighlightProtocol) -> Self {
        self.protocol = protocol;
        self
    }

    /// Set style modifiers for this rule.
    pub fn with_modifiers(mut self, modifiers: TerminalTokenModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Set foreground color override.
    pub fn with_foreground_color(mut self, color: impl Into<String>) -> Self {
        self.foreground_color = Some(color.into());
        self
    }

    /// Set background color override.
    pub fn with_background_color(mut self, color: impl Into<String>) -> Self {
        self.background_color = Some(color.into());
        self
    }
}

/// A semantic token representing a highlighted region in terminal output.
#[derive(Clone, Debug)]
pub struct SemanticToken {
    /// Line number (0-indexed)
    pub line: usize,
    /// Start column (0-indexed, in characters)
    pub start_col: usize,
    /// Length in characters
    pub length: usize,
    /// The token type
    pub token_type: TerminalTokenType,
    /// Style modifiers
    pub modifiers: TerminalTokenModifiers,
    /// Optional foreground color override
    pub foreground_color: Option<String>,
    /// Optional background color override
    pub background_color: Option<String>,
}

impl SemanticToken {
    /// Create a new semantic token.
    pub fn new(
        line: usize,
        start_col: usize,
        length: usize,
        token_type: TerminalTokenType,
    ) -> Self {
        Self {
            line,
            start_col,
            length,
            token_type,
            modifiers: TerminalTokenModifiers::default(),
            foreground_color: None,
            background_color: None,
        }
    }

    /// Set modifiers for this token.
    pub fn with_modifiers(mut self, modifiers: TerminalTokenModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Set foreground color override.
    pub fn with_foreground_color(mut self, color: Option<String>) -> Self {
        self.foreground_color = color;
        self
    }

    /// Set background color override.
    pub fn with_background_color(mut self, color: Option<String>) -> Self {
        self.background_color = color;
        self
    }

    /// Check if this token overlaps with a given position.
    pub fn contains(&self, line: usize, col: usize) -> bool {
        self.line == line && col >= self.start_col && col < self.start_col + self.length
    }

    /// Get the end column (exclusive).
    pub fn end_col(&self) -> usize {
        self.start_col + self.length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_modifiers() {
        let modifiers = TerminalTokenModifiers::new()
            .with_bold()
            .with_underline();

        assert!(modifiers.is_bold());
        assert!(!modifiers.is_italic());
        assert!(modifiers.is_underline());
        assert!(!modifiers.is_dim());
    }

    #[test]
    fn test_highlight_rule_serialization() {
        let rule = HighlightRule::new("Error", r"\bERROR\b", TerminalTokenType::Error)
            .with_case_insensitive(true)
            .with_priority(100);

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: HighlightRule = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "Error");
        assert_eq!(deserialized.pattern, r"\bERROR\b");
        assert!(deserialized.case_insensitive);
        assert_eq!(deserialized.priority, 100);
    }

    #[test]
    fn test_protocol_matching() {
        assert!(HighlightProtocol::All.matches(Some(&HighlightProtocol::Ssh)));
        assert!(HighlightProtocol::All.matches(Some(&HighlightProtocol::Telnet)));
        assert!(HighlightProtocol::All.matches(None));

        assert!(HighlightProtocol::Ssh.matches(Some(&HighlightProtocol::Ssh)));
        assert!(!HighlightProtocol::Ssh.matches(Some(&HighlightProtocol::Telnet)));

        assert!(HighlightProtocol::Local.matches(None));
    }

    #[test]
    fn test_semantic_token_contains() {
        let token = SemanticToken::new(5, 10, 5, TerminalTokenType::Error);

        assert!(token.contains(5, 10));
        assert!(token.contains(5, 14));
        assert!(!token.contains(5, 15));
        assert!(!token.contains(5, 9));
        assert!(!token.contains(4, 10));
    }
}
