use std::fs;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Events that can trigger rule evaluation.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerEvent {
    #[default]
    Wakeup,
    Connected,
    Disconnected,
}

/// Protocol type for connection-based conditions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Ssh,
    Telnet,
}

/// Type of credential to send.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialType {
    Username,
    Password,
}

/// Condition that must be met for a rule to trigger.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleCondition {
    Pattern {
        pattern: String,
        #[serde(default)]
        case_insensitive: bool,
    },
    ConnectionType {
        protocol: Protocol,
    },
    All {
        conditions: Vec<RuleCondition>,
    },
    Any {
        conditions: Vec<RuleCondition>,
    },
}

/// Action to execute when a rule matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    SendText {
        text: String,
        #[serde(default = "default_true")]
        append_newline: bool,
    },
    SendCredential {
        credential_type: CredentialType,
    },
    RunPython {
        code: String,
    },
    Sequence {
        actions: Vec<RuleAction>,
    },
    Delay {
        milliseconds: u64,
    },
}

fn default_true() -> bool {
    true
}

/// An automation rule for terminal connections.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: TriggerEvent,
    pub max_triggers: Option<u32>,
    pub condition: RuleCondition,
    pub action: RuleAction,
}

impl AutomationRule {
    pub fn new(name: impl Into<String>, condition: RuleCondition, action: RuleAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            trigger: TriggerEvent::default(),
            max_triggers: None,
            condition,
            action,
        }
    }

    pub fn with_trigger(mut self, trigger: TriggerEvent) -> Self {
        self.trigger = trigger;
        self
    }

    pub fn with_max_triggers(mut self, max: u32) -> Self {
        self.max_triggers = Some(max);
        self
    }
}

/// The rule store containing all automation rules.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuleStore {
    pub version: u32,
    pub rules: Vec<AutomationRule>,
}

impl RuleStore {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            rules: Vec::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::with_defaults());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn with_defaults() -> Self {
        let mut store = Self::new();
        store.rules = default_rules();
        store
    }

    pub fn add_rule(&mut self, rule: AutomationRule) {
        self.rules.push(rule);
    }

    pub fn remove_rule(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.rules.iter().position(|r| r.id == id) {
            self.rules.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn find_rule(&self, id: Uuid) -> Option<&AutomationRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn find_rule_mut(&mut self, id: Uuid) -> Option<&mut AutomationRule> {
        self.rules.iter_mut().find(|r| r.id == id)
    }

    pub fn enabled_rules(&self) -> impl Iterator<Item = &AutomationRule> {
        self.rules.iter().filter(|r| r.enabled)
    }
}

/// Default auto-login rules for Telnet connections.
fn default_rules() -> Vec<AutomationRule> {
    vec![
        AutomationRule {
            id: Uuid::new_v4(),
            name: "Telnet Username Prompt".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: Some(1),
            condition: RuleCondition::All {
                conditions: vec![
                    RuleCondition::ConnectionType {
                        protocol: Protocol::Telnet,
                    },
                    RuleCondition::Pattern {
                        pattern: r"(?i)(username|login|user)\s*:".to_string(),
                        case_insensitive: true,
                    },
                ],
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Username,
            },
        },
        AutomationRule {
            id: Uuid::new_v4(),
            name: "Telnet Password Prompt".to_string(),
            enabled: true,
            trigger: TriggerEvent::Wakeup,
            max_triggers: Some(1),
            condition: RuleCondition::All {
                conditions: vec![
                    RuleCondition::ConnectionType {
                        protocol: Protocol::Telnet,
                    },
                    RuleCondition::Pattern {
                        pattern: r"(?i)password\s*:".to_string(),
                        case_insensitive: true,
                    },
                ],
            },
            action: RuleAction::SendCredential {
                credential_type: CredentialType::Password,
            },
        },
    ]
}

/// Events emitted by the rule store for UI subscription.
#[derive(Clone, Debug)]
pub enum RuleStoreEvent {
    Changed,
    RuleAdded(Uuid),
    RuleRemoved(Uuid),
}

/// Global marker for cx.global access.
pub struct GlobalRuleStore(pub Entity<RuleStoreEntity>);
impl Global for GlobalRuleStore {}

/// GPUI Entity wrapping RuleStore.
pub struct RuleStoreEntity {
    store: RuleStore,
    save_task: Option<Task<()>>,
}

impl EventEmitter<RuleStoreEvent> for RuleStoreEntity {}

impl RuleStoreEntity {
    /// Initialize global rule store on app startup.
    pub fn init(cx: &mut App) {
        let store = RuleStore::load_from_file(paths::terminal_rules_file())
            .unwrap_or_else(|err| {
                log::error!("Failed to load terminal rules: {}", err);
                RuleStore::with_defaults()
            });

        let entity = cx.new(|_| Self {
            store,
            save_task: None,
        });

        cx.set_global(GlobalRuleStore(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRuleStore>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalRuleStore>().map(|g| g.0.clone())
    }

    /// Read-only access to store.
    pub fn store(&self) -> &RuleStore {
        &self.store
    }

    /// Get all rules.
    pub fn rules(&self) -> &[AutomationRule] {
        &self.store.rules
    }

    /// Get enabled rules.
    pub fn enabled_rules(&self) -> Vec<&AutomationRule> {
        self.store.enabled_rules().collect()
    }

    /// Add a rule and trigger save.
    pub fn add_rule(&mut self, rule: AutomationRule, cx: &mut Context<Self>) {
        let id = rule.id;
        self.store.add_rule(rule);
        self.schedule_save(cx);
        cx.emit(RuleStoreEvent::RuleAdded(id));
        cx.notify();
    }

    /// Remove a rule and trigger save.
    pub fn remove_rule(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.store.remove_rule(id) {
            self.schedule_save(cx);
            cx.emit(RuleStoreEvent::RuleRemoved(id));
            cx.notify();
        }
    }

    /// Update a rule and trigger save.
    pub fn update_rule(
        &mut self,
        id: Uuid,
        update_fn: impl FnOnce(&mut AutomationRule),
        cx: &mut Context<Self>,
    ) {
        if let Some(rule) = self.store.find_rule_mut(id) {
            update_fn(rule);
            self.schedule_save(cx);
            cx.emit(RuleStoreEvent::Changed);
            cx.notify();
        }
    }

    /// Toggle a rule's enabled state.
    pub fn toggle_rule_enabled(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(rule) = self.store.find_rule_mut(id) {
            rule.enabled = !rule.enabled;
            self.schedule_save(cx);
            cx.emit(RuleStoreEvent::Changed);
            cx.notify();
        }
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = store.save_to_file(paths::terminal_rules_file()) {
                log::error!("Failed to save terminal rules: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_store_serialization_roundtrip() {
        let mut store = RuleStore::new();

        let rule = AutomationRule::new(
            "Test Rule",
            RuleCondition::Pattern {
                pattern: "test".to_string(),
                case_insensitive: false,
            },
            RuleAction::SendText {
                text: "hello".to_string(),
                append_newline: true,
            },
        );
        store.add_rule(rule);

        let json = serde_json::to_string_pretty(&store).expect("serialize");
        let restored: RuleStore = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.version, store.version);
        assert_eq!(restored.rules.len(), 1);
        assert_eq!(restored.rules[0].name, "Test Rule");
    }

    #[test]
    fn test_default_rules() {
        let store = RuleStore::with_defaults();
        assert_eq!(store.rules.len(), 2);
        assert!(store.rules.iter().any(|r| r.name == "Telnet Username Prompt"));
        assert!(store.rules.iter().any(|r| r.name == "Telnet Password Prompt"));
    }

    #[test]
    fn test_rule_condition_all() {
        let condition = RuleCondition::All {
            conditions: vec![
                RuleCondition::ConnectionType {
                    protocol: Protocol::Telnet,
                },
                RuleCondition::Pattern {
                    pattern: "login:".to_string(),
                    case_insensitive: true,
                },
            ],
        };

        let json = serde_json::to_string(&condition).expect("serialize");
        let restored: RuleCondition = serde_json::from_str(&json).expect("deserialize");

        match restored {
            RuleCondition::All { conditions } => {
                assert_eq!(conditions.len(), 2);
            }
            _ => panic!("Expected All condition"),
        }
    }

    #[test]
    fn test_rule_action_sequence() {
        let action = RuleAction::Sequence {
            actions: vec![
                RuleAction::Delay { milliseconds: 100 },
                RuleAction::SendCredential {
                    credential_type: CredentialType::Username,
                },
            ],
        };

        let json = serde_json::to_string(&action).expect("serialize");
        let restored: RuleAction = serde_json::from_str(&json).expect("deserialize");

        match restored {
            RuleAction::Sequence { actions } => {
                assert_eq!(actions.len(), 2);
            }
            _ => panic!("Expected Sequence action"),
        }
    }

    #[test]
    fn test_trigger_event_default() {
        let trigger: TriggerEvent = Default::default();
        assert_eq!(trigger, TriggerEvent::Wakeup);
    }
}
