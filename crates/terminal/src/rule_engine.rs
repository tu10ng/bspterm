use std::collections::HashMap;
use std::time::{Duration, Instant};

use regex::Regex;
use uuid::Uuid;

use crate::rule_store::{
    AutomationRule, CredentialType, Protocol, RuleAction, RuleCondition, TriggerEvent,
};
use crate::ConnectionInfo;

const COOLDOWN_DURATION: Duration = Duration::from_secs(2);

/// A compiled rule with pre-compiled regex patterns.
struct CompiledRule {
    rule: AutomationRule,
    compiled_patterns: Vec<CompiledPattern>,
    compiled_exclude_context: Option<(Regex, usize)>,
}

struct CompiledPattern {
    regex: Regex,
}

/// Information about a connection for rule matching.
#[derive(Clone, Debug)]
pub struct ConnectionContext {
    pub protocol: Protocol,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl From<&ConnectionInfo> for ConnectionContext {
    fn from(info: &ConnectionInfo) -> Self {
        match info {
            ConnectionInfo::Ssh {
                username, password, ..
            } => ConnectionContext {
                protocol: Protocol::Ssh,
                username: username.clone(),
                password: password.clone(),
            },
            ConnectionInfo::Telnet {
                username, password, ..
            } => ConnectionContext {
                protocol: Protocol::Telnet,
                username: username.clone(),
                password: password.clone(),
            },
        }
    }
}

/// Rule engine for matching patterns and executing actions.
pub struct RuleEngine {
    compiled_rules: Vec<CompiledRule>,
    connection_context: ConnectionContext,
    trigger_counts: HashMap<Uuid, u32>,
    last_trigger_times: HashMap<Uuid, Instant>,
}

impl RuleEngine {
    /// Create a new rule engine with the given connection info and rules.
    pub fn new(connection_info: &ConnectionInfo, rules: &[AutomationRule]) -> Self {
        let connection_context = ConnectionContext::from(connection_info);
        let compiled_rules = rules
            .iter()
            .filter_map(|rule| {
                if !rule.enabled {
                    return None;
                }
                let compiled_patterns = Self::compile_condition(&rule.condition);
                let compiled_exclude_context =
                    rule.exclude_context.as_ref().and_then(|ctx| {
                        let pattern_str = if ctx.case_insensitive {
                            format!("(?i){}", ctx.pattern)
                        } else {
                            ctx.pattern.clone()
                        };
                        match Regex::new(&pattern_str) {
                            Ok(regex) => Some((regex, ctx.lines_before)),
                            Err(err) => {
                                log::warn!(
                                    "Failed to compile exclude_context pattern '{}': {}",
                                    ctx.pattern,
                                    err
                                );
                                None
                            }
                        }
                    });
                Some(CompiledRule {
                    rule: rule.clone(),
                    compiled_patterns,
                    compiled_exclude_context,
                })
            })
            .collect();

        Self {
            compiled_rules,
            connection_context,
            trigger_counts: HashMap::new(),
            last_trigger_times: HashMap::new(),
        }
    }

    fn compile_condition(condition: &RuleCondition) -> Vec<CompiledPattern> {
        match condition {
            RuleCondition::Pattern {
                pattern,
                case_insensitive,
            } => {
                let pattern_str = if *case_insensitive {
                    format!("(?i){}", pattern)
                } else {
                    pattern.clone()
                };
                match Regex::new(&pattern_str) {
                    Ok(regex) => vec![CompiledPattern { regex }],
                    Err(err) => {
                        log::warn!("Failed to compile pattern '{}': {}", pattern, err);
                        vec![]
                    }
                }
            }
            RuleCondition::All { conditions } => {
                conditions.iter().flat_map(Self::compile_condition).collect()
            }
            RuleCondition::Any { conditions } => {
                conditions.iter().flat_map(Self::compile_condition).collect()
            }
            RuleCondition::ConnectionType { .. } => vec![],
        }
    }

    /// Check rules for the given trigger event and screen content.
    /// `current_line_buffer` is the user's current input on the cursor line,
    /// used to strip user input from matching when `exclude_user_input` is enabled.
    /// `previous_lines` are the lines above the cursor (top-to-bottom order),
    /// used for context exclusion checking.
    /// Returns a list of actions to execute.
    pub fn check(
        &mut self,
        trigger: TriggerEvent,
        screen_content: &str,
        current_line_buffer: &str,
        previous_lines: &[String],
    ) -> Vec<MatchedAction> {
        let mut actions = Vec::new();
        let now = Instant::now();

        for compiled_rule in &self.compiled_rules {
            let rule = &compiled_rule.rule;

            if rule.trigger != trigger {
                continue;
            }

            if let Some(max) = rule.max_triggers {
                let count = self.trigger_counts.get(&rule.id).copied().unwrap_or(0);
                if count >= max {
                    continue;
                }
            }

            if let Some(last_time) = self.last_trigger_times.get(&rule.id) {
                if now.duration_since(*last_time) < COOLDOWN_DURATION {
                    continue;
                }
            }

            let content_to_match = if rule.exclude_user_input {
                strip_user_input(screen_content, current_line_buffer)
            } else {
                screen_content.to_string()
            };

            if self.matches_condition(
                &rule.condition,
                &compiled_rule.compiled_patterns,
                &content_to_match,
            ) {
                if let Some((exclude_regex, lines_before)) =
                    &compiled_rule.compiled_exclude_context
                {
                    let check_count = (*lines_before).min(previous_lines.len());
                    let start = previous_lines.len().saturating_sub(check_count);
                    if previous_lines[start..].iter().any(|line| exclude_regex.is_match(line)) {
                        log::debug!(
                            "Rule '{}' skipped by exclude_context match in previous lines",
                            rule.name
                        );
                        continue;
                    }
                }

                *self.trigger_counts.entry(rule.id).or_insert(0) += 1;
                self.last_trigger_times.insert(rule.id, now);

                actions.push(MatchedAction {
                    rule_id: rule.id,
                    rule_name: rule.name.clone(),
                    action: rule.action.clone(),
                });
            }
        }

        actions
    }

    /// Return the max `lines_before` across all compiled rules that have exclude_context.
    /// Returns 0 if no rule uses context exclusion.
    pub fn max_context_lines(&self) -> usize {
        self.compiled_rules
            .iter()
            .filter_map(|cr| cr.compiled_exclude_context.as_ref().map(|(_, lines)| *lines))
            .max()
            .unwrap_or(0)
    }

    fn matches_condition(
        &self,
        condition: &RuleCondition,
        compiled_patterns: &[CompiledPattern],
        screen_content: &str,
    ) -> bool {
        match condition {
            RuleCondition::Pattern { .. } => {
                compiled_patterns.iter().any(|p| p.regex.is_match(screen_content))
            }
            RuleCondition::ConnectionType { protocol } => {
                *protocol == self.connection_context.protocol
            }
            RuleCondition::All { conditions } => {
                conditions.iter().all(|c| {
                    let sub_patterns = Self::compile_condition(c);
                    self.matches_condition(c, &sub_patterns, screen_content)
                })
            }
            RuleCondition::Any { conditions } => {
                conditions.iter().any(|c| {
                    let sub_patterns = Self::compile_condition(c);
                    self.matches_condition(c, &sub_patterns, screen_content)
                })
            }
        }
    }

    /// Get the credential value for the given type.
    pub fn get_credential(&self, credential_type: &CredentialType) -> Option<String> {
        match credential_type {
            CredentialType::Username => self.connection_context.username.clone(),
            CredentialType::Password => self.connection_context.password.clone(),
        }
    }

    /// Reset trigger counts (useful for reconnection).
    pub fn reset_counts(&mut self) {
        self.trigger_counts.clear();
        self.last_trigger_times.clear();
    }
}

/// Strip user input from the end of the cursor line content.
/// If `line_buffer` is empty or doesn't match the suffix, returns the original content.
fn strip_user_input(cursor_line: &str, line_buffer: &str) -> String {
    if line_buffer.is_empty() {
        return cursor_line.to_string();
    }
    if let Some(stripped) = cursor_line.strip_suffix(line_buffer) {
        stripped.to_string()
    } else {
        cursor_line.to_string()
    }
}

/// A matched action ready to be executed.
#[derive(Clone, Debug)]
pub struct MatchedAction {
    pub rule_id: Uuid,
    pub rule_name: String,
    pub action: RuleAction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_store::ContextExclusion;

    fn make_telnet_connection_info() -> ConnectionInfo {
        ConnectionInfo::Telnet {
            host: "192.168.1.1".to_string(),
            port: 23,
            username: Some("admin".to_string()),
            password: Some("secret123".to_string()),
            session_id: None,
        }
    }

    fn make_ssh_connection_info() -> ConnectionInfo {
        ConnectionInfo::Ssh {
            host: "192.168.1.1".to_string(),
            port: 22,
            username: Some("root".to_string()),
            password: Some("password".to_string()),
            private_key_path: None,
            passphrase: None,
            session_id: None,
        }
    }

    #[test]
    fn test_pattern_matching() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Test Pattern".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"Please enter login:", "", &[]);
        assert_eq!(actions.len(), 1);

        let actions = engine.check(TriggerEvent::Wakeup,"No prompt here", "", &[]);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Case Insensitive".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"LOGIN:", "", &[]);
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"Login:", "", &[]);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_connection_type_condition() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![
            AutomationRule {
                id: Uuid::new_v4(),
                name: "Telnet Only".to_string(),
                enabled: true,
                trigger: TriggerEvent::Wakeup,
                max_triggers: None,
                condition: RuleCondition::All {
                    conditions: vec![
                        RuleCondition::ConnectionType {
                            protocol: Protocol::Telnet,
                        },
                        RuleCondition::Pattern {
                            pattern: "prompt".to_string(),
                            case_insensitive: false,
                        },
                    ],
                },
                action: RuleAction::SendText {
                    text: "test".to_string(),
                    append_newline: true,
                },
                exclude_user_input: true,
                exclude_context: None,
            },
            AutomationRule {
                id: Uuid::new_v4(),
                name: "SSH Only".to_string(),
                enabled: true,
                trigger: TriggerEvent::Wakeup,
                max_triggers: None,
                condition: RuleCondition::All {
                    conditions: vec![
                        RuleCondition::ConnectionType {
                            protocol: Protocol::Ssh,
                        },
                        RuleCondition::Pattern {
                            pattern: "prompt".to_string(),
                            case_insensitive: false,
                        },
                    ],
                },
                action: RuleAction::SendText {
                    text: "ssh_test".to_string(),
                    append_newline: true,
                },
                exclude_user_input: true,
                exclude_context: None,
            },
        ];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"Enter prompt:", "", &[]);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_name, "Telnet Only");
    }

    #[test]
    fn test_max_triggers() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Limited".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: Some(1),
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_trigger_event_filtering() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Connected Only".to_string(),
            enabled: true,
            trigger: TriggerEvent::Connected,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: ".*".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"anything", "", &[]);
        assert_eq!(actions.len(), 0);

        let actions = engine.check(TriggerEvent::Connected, "anything", "", &[]);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_get_credential() {
        let conn_info = make_telnet_connection_info();
        let engine = RuleEngine::new(&conn_info, &[]);

        assert_eq!(
            engine.get_credential(&CredentialType::Username),
            Some("admin".to_string())
        );
        assert_eq!(
            engine.get_credential(&CredentialType::Password),
            Some("secret123".to_string())
        );
    }

    #[test]
    fn test_disabled_rules_ignored() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Disabled".to_string(),
            enabled: false,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_any_condition() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Any Match".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Any {
                conditions: vec![
                    RuleCondition::Pattern {
                        pattern: "login:".to_string(),
                        case_insensitive: false,
                    },
                    RuleCondition::Pattern {
                        pattern: "username:".to_string(),
                        case_insensitive: false,
                    },
                ],
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"Enter username:", "", &[]);
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"Enter login:", "", &[]);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_reset_counts() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Limited".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: Some(1),
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);

        engine.reset_counts();

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_ssh_auto_login_rules() {
        let conn_info = make_ssh_connection_info();
        let rules = vec![
            AutomationRule {
                id: Uuid::new_v4(),
                name: "Auto Login - Username".to_string(),
                enabled: true,
                trigger: TriggerEvent::Wakeup,
                max_triggers: None,
                condition: RuleCondition::Pattern {
                    pattern: r"(?i)(username|login|user)\s*:".to_string(),
                    case_insensitive: true,
                },
                action: RuleAction::SendCredential {
                    credential_type: CredentialType::Username,
                },
                exclude_user_input: true,
                exclude_context: None,
            },
            AutomationRule {
                id: Uuid::new_v4(),
                name: "Auto Login - Password".to_string(),
                enabled: true,
                trigger: TriggerEvent::Wakeup,
                max_triggers: None,
                condition: RuleCondition::Pattern {
                    pattern: r"(?i)password\s*:".to_string(),
                    case_insensitive: true,
                },
                action: RuleAction::SendCredential {
                    credential_type: CredentialType::Password,
                },
                exclude_user_input: true,
                exclude_context: None,
            },
        ];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_name, "Auto Login - Username");
        assert_eq!(
            engine.get_credential(&CredentialType::Username),
            Some("root".to_string())
        );

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"Password:", "", &[]);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_name, "Auto Login - Password");
        assert_eq!(
            engine.get_credential(&CredentialType::Password),
            Some("password".to_string())
        );
    }

    #[test]
    fn test_fingerprint_accept_rule() {
        let conn_info = make_ssh_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "SSH Fingerprint Accept".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)continue connecting.*\(yes/no".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendText {
                text: "yes".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(
            TriggerEvent::Wakeup,
            "Are you sure you want to continue connecting (yes/no)?",
            "",
            &[],
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_name, "SSH Fingerprint Accept");

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(
            TriggerEvent::Wakeup,
            "Are you sure you want to continue connecting (yes/no/[fingerprint])?",
            "",
            &[],
        );
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"normal output without fingerprint", "", &[]);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_no_max_triggers_fires_repeatedly() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Unlimited".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "login:".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "test".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup,"login:", "", &[]);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_protocol_agnostic_rules() {
        let rule = AutomationRule {
            id: Uuid::new_v4(),
            name: "Protocol Agnostic".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)(username|login|user)\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Username,
            },
            exclude_user_input: true,
            exclude_context: None,
        };

        let telnet_info = make_telnet_connection_info();
        let mut telnet_engine = RuleEngine::new(&telnet_info, std::slice::from_ref(&rule));
        let actions = telnet_engine.check(TriggerEvent::Wakeup,"Username:", "", &[]);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            telnet_engine.get_credential(&CredentialType::Username),
            Some("admin".to_string())
        );

        let ssh_info = make_ssh_connection_info();
        let mut ssh_engine = RuleEngine::new(&ssh_info, std::slice::from_ref(&rule));
        let actions = ssh_engine.check(TriggerEvent::Wakeup,"Username:", "", &[]);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            ssh_engine.get_credential(&CredentialType::Username),
            Some("root".to_string())
        );
    }

    #[test]
    fn test_exclude_user_input_strips_echo() {
        let conn_info = make_ssh_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password Rule".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // User typed "cat /etc/passwd" and the cursor line shows the echo.
        // With exclude_user_input, the user input is stripped, leaving just the prompt.
        let actions = engine.check(
            TriggerEvent::Wakeup,
            "$ cat /etc/passwd",
            "cat /etc/passwd",
            &[],
        );
        assert_eq!(actions.len(), 0, "should not match when user input contains 'password'");
    }

    #[test]
    fn test_exclude_user_input_empty_buffer_matches_normally() {
        let conn_info = make_ssh_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password Rule".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // Empty buffer means server prompt, should match normally
        let actions = engine.check(TriggerEvent::Wakeup,"Password:", "", &[]);
        assert_eq!(actions.len(), 1, "should match server password prompt with empty buffer");
    }

    #[test]
    fn test_exclude_user_input_disabled_matches_full_content() {
        let conn_info = make_ssh_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password Rule".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: false,
            exclude_context: None,
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // With exclude_user_input=false, matches against full content including user echo
        let actions = engine.check(
            TriggerEvent::Wakeup,
            "$ grep password: config.txt",
            "grep password: config.txt",
            &[],
        );
        assert_eq!(actions.len(), 1, "should match full content when exclude_user_input is false");
    }

    #[test]
    fn test_strip_user_input_function() {
        // Empty buffer returns original
        assert_eq!(strip_user_input("Password:", ""), "Password:");

        // Matching suffix is stripped
        assert_eq!(strip_user_input("$ cat /etc/passwd", "cat /etc/passwd"), "$ ");

        // Non-matching suffix returns original
        assert_eq!(strip_user_input("Password:", "something_else"), "Password:");

        // Full match (user typed everything on the line)
        assert_eq!(strip_user_input("cat /etc/passwd", "cat /etc/passwd"), "");
    }

    #[test]
    fn test_exclude_context_skips_rule_on_match() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password with context exclusion".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: r"(?i)(bootload|bootrom|boot\s*menu|ftp)".to_string(),
                case_insensitive: true,
                lines_before: 5,
            }),
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // Bootloader context: previous lines contain "bootload Menu", should be skipped
        let previous_lines = vec![
            "Press Ctrl+B to enter bootload Menu or Ctrl+R to reset".to_string(),
            "".to_string(),
            "Bootloader version: 1.2.3".to_string(),
        ];
        let actions = engine.check(TriggerEvent::Wakeup, "Password:", "", &previous_lines);
        assert_eq!(actions.len(), 0, "should skip rule when bootload context found");
    }

    #[test]
    fn test_exclude_context_allows_rule_without_match() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password with context exclusion".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: r"(?i)(bootload|bootrom|boot\s*menu|ftp)".to_string(),
                case_insensitive: true,
                lines_before: 5,
            }),
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // Normal login context: no bootload text in previous lines
        let previous_lines = vec![
            "Welcome to SSH server".to_string(),
            "Last login: Mon Mar 17 10:00:00 2026".to_string(),
        ];
        let actions = engine.check(TriggerEvent::Wakeup, "Password:", "", &previous_lines);
        assert_eq!(actions.len(), 1, "should trigger rule when no exclusion context found");
    }

    #[test]
    fn test_exclude_context_empty_previous_lines() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password with context exclusion".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: r"(?i)(bootload|bootrom|boot\s*menu|ftp)".to_string(),
                case_insensitive: true,
                lines_before: 5,
            }),
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // No previous lines (e.g., cursor at top of screen)
        let actions = engine.check(TriggerEvent::Wakeup, "Password:", "", &[]);
        assert_eq!(actions.len(), 1, "should trigger when no previous lines available");
    }

    #[test]
    fn test_exclude_context_ftp_context() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password with context exclusion".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: r"(?i)(bootload|bootrom|boot\s*menu|ftp)".to_string(),
                case_insensitive: true,
                lines_before: 5,
            }),
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // FTP context
        let previous_lines = vec![
            "Connected to ftp.example.com".to_string(),
            "220 FTP server ready".to_string(),
            "User admin OK".to_string(),
        ];
        let actions = engine.check(TriggerEvent::Wakeup, "Password:", "", &previous_lines);
        assert_eq!(actions.len(), 0, "should skip rule when ftp context found");
    }

    #[test]
    fn test_exclude_context_respects_lines_before() {
        let conn_info = make_telnet_connection_info();
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Password with small window".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: r"(?i)password\s*:".to_string(),
                case_insensitive: true,
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: r"(?i)bootload".to_string(),
                case_insensitive: true,
                lines_before: 2,
            }),
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        // Bootload text is 4 lines back, but lines_before is only 2
        let previous_lines = vec![
            "bootload menu".to_string(),
            "some line".to_string(),
            "another line".to_string(),
            "yet another".to_string(),
        ];
        let actions = engine.check(TriggerEvent::Wakeup, "Password:", "", &previous_lines);
        assert_eq!(
            actions.len(), 1,
            "should trigger because bootload is beyond lines_before window"
        );
    }

    #[test]
    fn test_max_context_lines() {
        let conn_info = make_telnet_connection_info();

        // No rules with exclude_context
        let rules = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Simple rule".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "test".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "hello".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: None,
        }];
        let engine = RuleEngine::new(&conn_info, &rules);
        assert_eq!(engine.max_context_lines(), 0);

        // Rule with exclude_context
        let rules_with_context = vec![AutomationRule {
            id: Uuid::new_v4(),
            name: "Context rule".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: None,
            condition: RuleCondition::Pattern {
                pattern: "test".to_string(),
                case_insensitive: false,
            },
            action: RuleAction::SendText {
                text: "hello".to_string(),
                append_newline: true,
            },
            exclude_user_input: true,
            exclude_context: Some(ContextExclusion {
                pattern: "exclude_me".to_string(),
                case_insensitive: false,
                lines_before: 7,
            }),
        }];
        let engine = RuleEngine::new(&conn_info, &rules_with_context);
        assert_eq!(engine.max_context_lines(), 7);
    }
}
