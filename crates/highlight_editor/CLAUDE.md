# highlight_editor

Panel for managing terminal semantic highlighting rules (regex-based pattern matching with colors).

## Module Structure

```
src/
├── highlight_editor.rs      # Main panel with rule list
└── highlight_edit_modal.rs  # Modal for creating/editing rules
```

## Key Types

| Type | Purpose |
|------|---------|
| `HighlightEditor` | Main dockable panel component |
| `HighlightEditModal` | Modal dialog for rule editing |

## Dependencies

- `terminal` - HighlightStore, HighlightRule, TerminalTokenType, TerminalTokenModifiers
- `workspace` - Panel, ModalView, DockPosition
- `editor` - Text input fields
- `ui` - Button, ListItem, Checkbox, Label
- `regex` - Live pattern validation

## Common Tasks

**Add new token type:**
1. Add variant to `TerminalTokenType` in `terminal/highlight_rule.rs`
2. Add button in token type selector in `highlight_edit_modal.rs`

**Add new modifier:**
1. Add constant to `TerminalTokenModifiers` in `terminal/highlight_rule.rs`
2. Add checkbox in modifiers section in `highlight_edit_modal.rs`

## Testing

```sh
cargo test -p highlight_editor
```

## Pitfalls

- Rule list uses UniformList for virtualized scrolling
- Subscribes to `HighlightStoreEvent` for real-time updates
- Token type selector uses button group with 10 options (wraps on narrow panels)
- Protocol selector uses button group (All, SSH, Telnet, Local)
- Regex validation happens on every pattern/sample text change
- Save button disabled when regex is invalid
- Panel docks on Right side by default (priority: 12)
- Uses Sparkle icon to distinguish from Rule Editor (Cog icon)
