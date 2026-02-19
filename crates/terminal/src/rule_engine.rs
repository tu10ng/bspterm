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
                Some(CompiledRule {
                    rule: rule.clone(),
                    compiled_patterns,
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
    /// Returns a list of actions to execute.
    pub fn check(&mut self, trigger: TriggerEvent, screen_content: &str) -> Vec<MatchedAction> {
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

            if self.matches_condition(
                &rule.condition,
                &compiled_rule.compiled_patterns,
                screen_content,
            ) {
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

    fn make_telnet_connection_info() -> ConnectionInfo {
        ConnectionInfo::Telnet {
            host: "192.168.1.1".to_string(),
            port: 23,
            username: Some("admin".to_string()),
            password: Some("secret123".to_string()),
            session_id: None,
        }
    }

    #[allow(dead_code)]
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "Please enter login:");
        assert_eq!(actions.len(), 1);

        let actions = engine.check(TriggerEvent::Wakeup, "No prompt here");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "LOGIN:");
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup, "Login:");
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
            },
        ];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "Enter prompt:");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "login:");
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup, "login:");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "anything");
        assert_eq!(actions.len(), 0);

        let actions = engine.check(TriggerEvent::Connected, "anything");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "login:");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "Enter username:");
        assert_eq!(actions.len(), 1);

        std::thread::sleep(Duration::from_millis(2100));

        let actions = engine.check(TriggerEvent::Wakeup, "Enter login:");
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
        }];

        let mut engine = RuleEngine::new(&conn_info, &rules);

        let actions = engine.check(TriggerEvent::Wakeup, "login:");
        assert_eq!(actions.len(), 1);

        engine.reset_counts();

        let actions = engine.check(TriggerEvent::Wakeup, "login:");
        assert_eq!(actions.len(), 1);
    }
}
