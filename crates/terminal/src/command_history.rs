use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A command extracted from terminal output.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalCommand {
    /// The command text (portion after the prompt).
    pub command_text: String,
    /// The full prompt prefix (e.g., "[Huawei-aaa]", "root@host:~$").
    pub prompt: String,
    /// The line number in the terminal grid (can be negative for scrollback).
    pub line: i32,
    /// The timestamp when this command was executed.
    pub timestamp: Option<DateTime<Local>>,
}

/// Manages command history extracted from terminal output.
#[derive(Debug, Default)]
pub struct CommandHistory {
    commands: Vec<TerminalCommand>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Returns all recorded commands.
    pub fn commands(&self) -> &[TerminalCommand] {
        &self.commands
    }

    /// Clears all commands from history.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Adds a command directly to the history (used when Enter is pressed).
    pub fn add_command(
        &mut self,
        command_text: String,
        prompt: String,
        line: i32,
        timestamp: DateTime<Local>,
    ) {
        self.commands.push(TerminalCommand {
            command_text,
            prompt,
            line,
            timestamp: Some(timestamp),
        });
    }

    /// Processes a line of terminal output, extracting any command if present.
    /// Returns true if a new command was extracted.
    pub fn process_line(
        &mut self,
        line_content: &str,
        line_number: i32,
        timestamp: Option<DateTime<Local>>,
    ) -> bool {
        if let Some((prompt, command)) = extract_command(line_content) {
            if !command.is_empty() {
                self.commands.push(TerminalCommand {
                    command_text: command,
                    prompt,
                    line: line_number,
                    timestamp,
                });
                return true;
            }
        }
        false
    }

    /// Adjusts line numbers when terminal content scrolls.
    /// scroll_delta is the number of lines that scrolled up (positive = scrolled up).
    pub fn adjust_for_scroll(&mut self, scroll_delta: i32, topmost_line: i32) {
        if scroll_delta > 0 {
            for cmd in &mut self.commands {
                cmd.line -= scroll_delta;
            }
            self.commands.retain(|cmd| cmd.line >= topmost_line);
        }
    }

    /// Removes commands whose line numbers are below the given threshold.
    pub fn cleanup_old_commands(&mut self, topmost_line: i32) {
        self.commands.retain(|cmd| cmd.line >= topmost_line);
    }
}

/// Static compiled regex patterns for prompt detection.
fn prompt_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // Standard Unix prompts ending with $, #, or >
            // e.g., "user@host:~$ ", "root# ", "bash-5.1$ "
            Regex::new(r"^(.*?[$#>])\s+(.+)$").unwrap(),
            // Huawei user view: <DeviceName>command
            // e.g., "<Huawei>display version"
            Regex::new(r"^(<[^<>]+>)(.+)$").unwrap(),
            // Huawei system/sub-view: [DeviceName] or [DeviceName-xxx]
            // e.g., "[Huawei]interface GigabitEthernet0/0/1"
            // e.g., "[Huawei-GigabitEthernet0/0/1]ip address 192.168.1.1 24"
            Regex::new(r"^(\[[^\[\]]+\])(.+)$").unwrap(),
            // Cisco configuration modes: Router(config)#, Router(config-if)#
            // e.g., "Router(config)#hostname R1"
            Regex::new(r"^(.*\([^()]+\)[#>])\s*(.+)$").unwrap(),
            // BusyBox/embedded: simple prompts like "/ # ", "~ $ "
            Regex::new(r"^([~/][^$#>]*[$#>])\s+(.+)$").unwrap(),
        ]
    })
}

/// Extracts the prompt and command from a line of terminal output.
/// Returns (prompt, command) if a valid prompt pattern is found with a non-empty command.
pub(crate) fn extract_command(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    for pattern in prompt_patterns() {
        if let Some(caps) = pattern.captures(trimmed) {
            if let (Some(prompt_match), Some(command_match)) = (caps.get(1), caps.get(2)) {
                let prompt = prompt_match.as_str().to_string();
                let command = command_match.as_str().trim().to_string();
                if !command.is_empty() {
                    return Some((prompt, command));
                }
            }
        }
    }

    None
}

/// Like `extract_command`, but preserves trailing whitespace in the command.
/// Used by autosuggestion to keep the user's trailing spaces for accurate prefix matching.
pub(crate) fn extract_command_preserve_trailing(line: &str) -> Option<(String, String)> {
    if line.trim().is_empty() {
        return None;
    }

    for pattern in prompt_patterns() {
        if let Some(caps) = pattern.captures(line) {
            if let (Some(prompt_match), Some(command_match)) = (caps.get(1), caps.get(2)) {
                let prompt = prompt_match.as_str().to_string();
                let command = command_match.as_str().to_string();
                if !command.trim().is_empty() {
                    return Some((prompt, command));
                }
            }
        }
    }

    None
}

// ── PromptContext classification ─────────────────────────────────────────

/// Classifies the type of prompt to scope autosuggestion matching.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PromptContext {
    UnixShell,
    HuaweiUserView,
    HuaweiSystemView { sub_view: Option<String> },
    CiscoConfig { mode: String },
    Unknown,
}

/// Classifies a prompt string into a PromptContext.
pub fn classify_prompt(prompt: &str) -> PromptContext {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return PromptContext::Unknown;
    }

    // <DeviceName> → HuaweiUserView
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        return PromptContext::HuaweiUserView;
    }

    // [DeviceName] or [DeviceName-xxx] → HuaweiSystemView
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let sub_view = if let Some(pos) = inner.find('-') {
            let raw = &inner[pos + 1..];
            Some(normalize_sub_view(raw))
        } else {
            None
        };
        return PromptContext::HuaweiSystemView { sub_view };
    }

    // hostname(config)# or hostname(config-if)> → CiscoConfig
    if let Some(paren_start) = trimmed.rfind('(') {
        if let Some(paren_end) = trimmed[paren_start..].find(')') {
            let after_paren = &trimmed[paren_start + paren_end + 1..];
            if after_paren == "#" || after_paren == ">" {
                let mode = trimmed[paren_start + 1..paren_start + paren_end].to_string();
                return PromptContext::CiscoConfig { mode };
            }
        }
    }

    // Ends with $ or # or > → UnixShell
    if trimmed.ends_with('$') || trimmed.ends_with('#') || trimmed.ends_with('>') {
        return PromptContext::UnixShell;
    }

    PromptContext::Unknown
}

/// Normalizes a Huawei sub-view name by stripping trailing digits and separators.
/// e.g., "GigabitEthernet0/0/1" → "GigabitEthernet", "Vlanif10" → "Vlanif"
pub fn normalize_sub_view(raw: &str) -> String {
    let trimmed = raw.trim_end_matches(|c: char| c.is_ascii_digit() || c == '/' || c == '.');
    if trimmed.is_empty() {
        raw.to_string()
    } else {
        trimmed.to_string()
    }
}

// ── GlobalCommandPool ──────────────────────────────────────────────────

/// A command stored with its prompt context for scoped autosuggestion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextualCommand {
    pub context: PromptContext,
    pub command: String,
    /// Unix timestamp of when this command was last used.
    /// `None` for entries saved before this field existed (backward compat).
    #[serde(default)]
    pub last_used: Option<i64>,
}

fn default_max_commands() -> usize {
    10_000
}

/// Global pool of commands from all terminal instances, persisted to disk.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlobalCommandPool {
    commands: Vec<ContextualCommand>,
    #[serde(skip, default = "default_max_commands")]
    max_commands: usize,
}

impl Default for GlobalCommandPool {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            max_commands: default_max_commands(),
        }
    }
}

impl GlobalCommandPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command to the pool. Deduplicates by (context, command) — moves existing to front.
    pub fn add_command(&mut self, command: String, context: PromptContext) {
        if command.is_empty() {
            return;
        }
        let now = Utc::now().timestamp();
        // Remove duplicate if exists
        self.commands
            .retain(|entry| !(entry.context == context && entry.command == command));
        // Insert at front (most recent first)
        self.commands.insert(
            0,
            ContextualCommand {
                context,
                command,
                last_used: Some(now),
            },
        );
        // Enforce max limit
        if self.commands.len() > self.max_commands {
            self.commands.truncate(self.max_commands);
        }
    }

    /// Find a suggestion for a prefix within the same prompt context.
    /// Returns the full command (caller can derive the suffix).
    pub fn find_suggestion(&self, prefix: &str, context: &PromptContext) -> Option<&str> {
        if prefix.is_empty() {
            return None;
        }
        self.commands
            .iter()
            .find(|entry| entry.context == *context && entry.command.starts_with(prefix) && entry.command != prefix)
            .map(|entry| entry.command.as_str())
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut pool: Self = serde_json::from_str(&content)?;
        pool.max_commands = default_max_commands();
        Ok(pool)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Remove commands older than `max_age_days`.
    /// Entries with `last_used == None` (pre-upgrade) get stamped with `now` to survive one cycle.
    pub fn purge_expired(&mut self, max_age_days: u64) {
        let now = Utc::now().timestamp();
        let cutoff = now - (max_age_days as i64) * 86400;

        // Backfill legacy entries that have no timestamp
        for entry in &mut self.commands {
            if entry.last_used.is_none() {
                entry.last_used = Some(now);
            }
        }

        self.commands
            .retain(|entry| entry.last_used.unwrap_or(now) >= cutoff);
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn commands(&self) -> &[ContextualCommand] {
        &self.commands
    }

    #[cfg(test)]
    pub fn with_max_commands(mut self, max: usize) -> Self {
        self.max_commands = max;
        self
    }

    #[cfg(test)]
    pub fn push_raw(&mut self, entry: ContextualCommand) {
        self.commands.push(entry);
    }
}

// ── GPUI Entity wrapper ────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum GlobalCommandPoolEvent {
    Changed,
}

pub struct GlobalCommandPoolMarker(pub Entity<GlobalCommandPoolEntity>);
impl Global for GlobalCommandPoolMarker {}

pub struct GlobalCommandPoolEntity {
    pool: GlobalCommandPool,
    save_task: Option<Task<()>>,
}

impl EventEmitter<GlobalCommandPoolEvent> for GlobalCommandPoolEntity {}

impl GlobalCommandPoolEntity {
    pub fn init(cx: &mut App) {
        Self::init_with_max_age(cx, None);
    }

    pub fn init_with_max_age(cx: &mut App, max_age_days: Option<u64>) {
        if cx.try_global::<GlobalCommandPoolMarker>().is_some() {
            return;
        }

        let mut pool = GlobalCommandPool::load_from_file(paths::command_pool_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load command pool: {}", err);
                GlobalCommandPool::new()
            });

        let max_age = max_age_days.unwrap_or(7);
        pool.purge_expired(max_age);

        let entity = cx.new(|_| Self {
            pool,
            save_task: None,
        });

        cx.set_global(GlobalCommandPoolMarker(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalCommandPoolMarker>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalCommandPoolMarker>()
            .map(|g| g.0.clone())
    }

    pub fn add_command(
        &mut self,
        command: String,
        context: PromptContext,
        cx: &mut Context<Self>,
    ) {
        self.pool.add_command(command, context);
        self.schedule_save(cx);
    }

    pub fn find_suggestion(&self, prefix: &str, context: &PromptContext) -> Option<&str> {
        self.pool.find_suggestion(prefix, context)
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.pool.clear();
        self.schedule_save(cx);
        cx.emit(GlobalCommandPoolEvent::Changed);
    }

    pub fn pool_len(&self) -> usize {
        self.pool.commands().len()
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let pool = self.pool.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = pool.save_to_file(paths::command_pool_file()) {
                log::error!("Failed to save command pool: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_prompt_extraction() {
        // Standard bash prompt
        let result = extract_command("user@host:~$ ls -la");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "user@host:~$");
        assert_eq!(cmd, "ls -la");

        // Root prompt
        let result = extract_command("root@server:/var/log# tail -f syslog");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "root@server:/var/log#");
        assert_eq!(cmd, "tail -f syslog");
    }

    #[test]
    fn test_huawei_user_view() {
        let result = extract_command("<Huawei>display version");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "<Huawei>");
        assert_eq!(cmd, "display version");
    }

    #[test]
    fn test_huawei_system_view() {
        let result = extract_command("[Huawei]interface GigabitEthernet0/0/1");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "[Huawei]");
        assert_eq!(cmd, "interface GigabitEthernet0/0/1");
    }

    #[test]
    fn test_huawei_sub_view() {
        let result = extract_command("[Huawei-GigabitEthernet0/0/1]ip address 192.168.1.1 24");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "[Huawei-GigabitEthernet0/0/1]");
        assert_eq!(cmd, "ip address 192.168.1.1 24");
    }

    #[test]
    fn test_cisco_config_mode() {
        let result = extract_command("Router(config)#hostname R1");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "Router(config)#");
        assert_eq!(cmd, "hostname R1");

        let result = extract_command("Router(config-if)#ip address 10.0.0.1 255.255.255.0");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "Router(config-if)#");
        assert_eq!(cmd, "ip address 10.0.0.1 255.255.255.0");
    }

    #[test]
    fn test_busybox_prompt() {
        let result = extract_command("/ # ls");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "/ #");
        assert_eq!(cmd, "ls");

        let result = extract_command("~ $ cat /etc/passwd");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "~ $");
        assert_eq!(cmd, "cat /etc/passwd");
    }

    #[test]
    fn test_empty_command() {
        // Prompt only, no command - should return None
        let result = extract_command("user@host:~$ ");
        assert!(result.is_none());

        let result = extract_command("<Huawei>");
        assert!(result.is_none());
    }

    #[test]
    fn test_command_output_not_matched() {
        // Regular output lines should not be matched
        let result = extract_command("total 48");
        assert!(result.is_none());

        let result = extract_command("drwxr-xr-x 2 user user 4096 Jan 1 12:00 dir");
        assert!(result.is_none());
    }

    #[test]
    fn test_command_history_process_line() {
        let mut history = CommandHistory::new();
        let now = Local::now();

        let added = history.process_line("user@host:~$ ls -la", 0, Some(now));
        assert!(added);
        assert_eq!(history.commands().len(), 1);
        assert_eq!(history.commands()[0].command_text, "ls -la");
        assert_eq!(history.commands()[0].line, 0);

        let added = history.process_line("[Huawei]display ip routing-table", 5, Some(now));
        assert!(added);
        assert_eq!(history.commands().len(), 2);
        assert_eq!(history.commands()[1].command_text, "display ip routing-table");
    }

    #[test]
    fn test_command_history_scroll_adjustment() {
        let mut history = CommandHistory::new();
        let now = Local::now();

        history.process_line("user@host:~$ cmd1", 0, Some(now));
        history.process_line("user@host:~$ cmd2", 5, Some(now));
        history.process_line("user@host:~$ cmd3", 10, Some(now));

        // Simulate scrolling up by 3 lines
        history.adjust_for_scroll(3, -100);

        assert_eq!(history.commands()[0].line, -3);
        assert_eq!(history.commands()[1].line, 2);
        assert_eq!(history.commands()[2].line, 7);
    }

    #[test]
    fn test_command_history_cleanup() {
        let mut history = CommandHistory::new();
        let now = Local::now();

        history.process_line("user@host:~$ cmd1", -10, Some(now));
        history.process_line("user@host:~$ cmd2", -5, Some(now));
        history.process_line("user@host:~$ cmd3", 0, Some(now));

        // Clean up commands that scrolled past line -7
        history.cleanup_old_commands(-7);

        assert_eq!(history.commands().len(), 2);
        assert_eq!(history.commands()[0].command_text, "cmd2");
    }

    // ── PromptContext tests ────────────────────────────────────────────

    #[test]
    fn test_classify_prompt() {
        // Unix shell prompts
        assert_eq!(classify_prompt("user@host:~$"), PromptContext::UnixShell);
        assert_eq!(classify_prompt("root#"), PromptContext::UnixShell);
        assert_eq!(classify_prompt("/ #"), PromptContext::UnixShell);
        assert_eq!(classify_prompt("~ $"), PromptContext::UnixShell);

        // Huawei user view
        assert_eq!(classify_prompt("<Huawei>"), PromptContext::HuaweiUserView);
        assert_eq!(classify_prompt("<R1>"), PromptContext::HuaweiUserView);

        // Huawei system view
        assert_eq!(
            classify_prompt("[Huawei]"),
            PromptContext::HuaweiSystemView { sub_view: None }
        );
        assert_eq!(
            classify_prompt("[Huawei-GigabitEthernet0/0/1]"),
            PromptContext::HuaweiSystemView {
                sub_view: Some("GigabitEthernet".to_string())
            }
        );
        assert_eq!(
            classify_prompt("[Huawei-Vlanif10]"),
            PromptContext::HuaweiSystemView {
                sub_view: Some("Vlanif".to_string())
            }
        );

        // Cisco config
        assert_eq!(
            classify_prompt("Router(config)#"),
            PromptContext::CiscoConfig {
                mode: "config".to_string()
            }
        );
        assert_eq!(
            classify_prompt("Router(config-if)#"),
            PromptContext::CiscoConfig {
                mode: "config-if".to_string()
            }
        );

        // Unknown
        assert_eq!(classify_prompt(""), PromptContext::Unknown);
        assert_eq!(classify_prompt("something"), PromptContext::Unknown);
    }

    #[test]
    fn test_normalize_sub_view() {
        assert_eq!(normalize_sub_view("GigabitEthernet0/0/1"), "GigabitEthernet");
        assert_eq!(normalize_sub_view("Vlanif10"), "Vlanif");
        assert_eq!(normalize_sub_view("aaa"), "aaa");
        assert_eq!(normalize_sub_view("LoopBack0"), "LoopBack");
        // Edge case: all digits
        assert_eq!(normalize_sub_view("123"), "123");
    }

    #[test]
    fn test_contextual_suggestion() {
        let mut pool = GlobalCommandPool::new();
        pool.add_command("display version".to_string(), PromptContext::HuaweiUserView);
        pool.add_command("display ip routing-table".to_string(), PromptContext::HuaweiUserView);
        pool.add_command("ls -la".to_string(), PromptContext::UnixShell);

        // Same context → returns match
        assert_eq!(
            pool.find_suggestion("disp", &PromptContext::HuaweiUserView),
            Some("display ip routing-table")
        );

        // Different context → no match
        assert_eq!(
            pool.find_suggestion("disp", &PromptContext::UnixShell),
            None
        );

        // Exact match → no suggestion (need something longer)
        assert_eq!(
            pool.find_suggestion("ls -la", &PromptContext::UnixShell),
            None
        );

        // Empty prefix → no suggestion
        assert_eq!(
            pool.find_suggestion("", &PromptContext::UnixShell),
            None
        );
    }

    #[test]
    fn test_recency_ordering() {
        let mut pool = GlobalCommandPool::new();
        pool.add_command("display version".to_string(), PromptContext::HuaweiUserView);
        pool.add_command("display ip routing-table".to_string(), PromptContext::HuaweiUserView);

        // Most recent match first
        assert_eq!(
            pool.find_suggestion("display", &PromptContext::HuaweiUserView),
            Some("display ip routing-table")
        );

        // Re-add "display version" to move it to front
        pool.add_command("display version".to_string(), PromptContext::HuaweiUserView);
        assert_eq!(
            pool.find_suggestion("display", &PromptContext::HuaweiUserView),
            Some("display version")
        );
    }

    #[test]
    fn test_deduplication() {
        let mut pool = GlobalCommandPool::new();
        pool.add_command("ls -la".to_string(), PromptContext::UnixShell);
        pool.add_command("pwd".to_string(), PromptContext::UnixShell);
        pool.add_command("ls -la".to_string(), PromptContext::UnixShell);

        // Should have only 2 commands (deduplicated)
        assert_eq!(pool.commands().len(), 2);
        // "ls -la" should be at front (most recent)
        assert_eq!(pool.commands()[0].command, "ls -la");
        assert_eq!(pool.commands()[1].command, "pwd");
    }

    #[test]
    fn test_max_limit() {
        let mut pool = GlobalCommandPool::new().with_max_commands(3);
        pool.add_command("cmd1".to_string(), PromptContext::UnixShell);
        pool.add_command("cmd2".to_string(), PromptContext::UnixShell);
        pool.add_command("cmd3".to_string(), PromptContext::UnixShell);
        pool.add_command("cmd4".to_string(), PromptContext::UnixShell);

        assert_eq!(pool.commands().len(), 3);
        // Oldest (cmd1) should be evicted
        assert_eq!(pool.commands()[0].command, "cmd4");
        assert_eq!(pool.commands()[1].command, "cmd3");
        assert_eq!(pool.commands()[2].command, "cmd2");
    }

    #[test]
    fn test_add_command_sets_timestamp() {
        let mut pool = GlobalCommandPool::new();
        pool.add_command("ls -la".to_string(), PromptContext::UnixShell);

        let entry = &pool.commands()[0];
        assert!(entry.last_used.is_some());
        let now = Utc::now().timestamp();
        // Timestamp should be within 2 seconds of now
        assert!((now - entry.last_used.unwrap()).abs() < 2);
    }

    #[test]
    fn test_purge_expired_commands() {
        let mut pool = GlobalCommandPool::new();
        let now = Utc::now().timestamp();

        // Add a fresh command via add_command (gets current timestamp)
        pool.add_command("fresh".to_string(), PromptContext::UnixShell);

        // Manually insert an old command (8 days ago)
        pool.push_raw(ContextualCommand {
            context: PromptContext::UnixShell,
            command: "old_cmd".to_string(),
            last_used: Some(now - 8 * 86400),
        });

        // Manually insert a command with no timestamp (legacy)
        pool.push_raw(ContextualCommand {
            context: PromptContext::UnixShell,
            command: "legacy_cmd".to_string(),
            last_used: None,
        });

        assert_eq!(pool.commands().len(), 3);

        pool.purge_expired(7);

        // "fresh" survives, "legacy_cmd" gets backfilled and survives, "old_cmd" is purged
        assert_eq!(pool.commands().len(), 2);
        assert_eq!(pool.commands()[0].command, "fresh");
        assert_eq!(pool.commands()[1].command, "legacy_cmd");
        // Legacy entry should now have a timestamp
        assert!(pool.commands()[1].last_used.is_some());
    }

    #[test]
    fn test_clear_pool() {
        let mut pool = GlobalCommandPool::new();
        pool.add_command("cmd1".to_string(), PromptContext::UnixShell);
        pool.add_command("cmd2".to_string(), PromptContext::UnixShell);
        assert_eq!(pool.commands().len(), 2);

        pool.clear();
        assert_eq!(pool.commands().len(), 0);
    }

    #[test]
    fn test_extract_command_preserve_trailing() {
        // Trailing space preserved for autosuggestion prefix matching
        let result = extract_command_preserve_trailing("user@host:~$ ls ");
        assert!(result.is_some());
        let (prompt, cmd) = result.unwrap();
        assert_eq!(prompt, "user@host:~$");
        assert_eq!(cmd, "ls ");

        // No trailing space — same as extract_command
        let result = extract_command_preserve_trailing("user@host:~$ ls");
        assert!(result.is_some());
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd, "ls");

        // Multiple trailing spaces preserved
        let result = extract_command_preserve_trailing("user@host:~$ ls -la  ");
        assert!(result.is_some());
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd, "ls -la  ");

        // Prompt only (no command after spaces) — returns None
        let result = extract_command_preserve_trailing("user@host:~$   ");
        assert!(result.is_none());

        // Empty line — returns None
        let result = extract_command_preserve_trailing("   ");
        assert!(result.is_none());
    }
}
