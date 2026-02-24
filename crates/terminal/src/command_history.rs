use chrono::{DateTime, Local};
use regex::Regex;
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
fn extract_command(line: &str) -> Option<(String, String)> {
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
}
