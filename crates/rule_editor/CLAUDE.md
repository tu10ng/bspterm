# rule_editor

Panel for managing terminal automation rules (auto-login, pattern-based actions).

## Module Structure

```
src/
├── rule_editor.rs      # Main panel with rule list
└── rule_edit_modal.rs  # Modal for creating/editing rules
```

## Key Types

| Type | Purpose |
|------|---------|
| `RuleEditor` | Main dockable panel component |
| `RuleEditModal` | Modal dialog for rule editing |
| `ActionType` | Internal enum: SendCredential / SendText |

## Dependencies

- `terminal` - RuleStore, AutomationRule, TriggerEvent, RuleCondition, RuleAction
- `workspace` - Panel, ModalView, DockPosition
- `editor` - Text input fields
- `ui` - Button, ListItem, Checkbox

## Common Tasks

**Add new trigger event:**
1. Add variant to `TriggerEvent` in `terminal/rule_store.rs`
2. Add button in trigger selector in `rule_edit_modal.rs`

**Add new action type:**
1. Add variant to `RuleAction` in `terminal/rule_store.rs`
2. Update `ActionType` enum in `rule_edit_modal.rs`
3. Add UI controls for new action parameters

## Testing

```sh
cargo test -p rule_editor
```

## Pitfalls

- Rule list uses UniformList for virtualized scrolling
- Subscribes to `RuleStoreEvent` for real-time updates
- Trigger selector uses button group (Wakeup, Connected, Disconnected)
- Protocol selector uses button group (SSH, Telnet)
- Condition pattern supports case-sensitivity toggle
- Panel docks on Right side by default (priority: 11)
