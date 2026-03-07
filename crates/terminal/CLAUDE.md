# terminal

Core terminal emulation engine providing terminal rendering, SSH/Telnet connections, automation rules, and session management. Built on top of alacritty_terminal.

## Module Structure

```
src/
├── terminal.rs           # Main Terminal entity (184KB)
├── config_store.rs       # Generic ConfigItem/JsonConfigStore traits for store dedup
├── connection/           # Terminal connection backends
│   ├── mod.rs            # TerminalConnection trait, ConnectionState
│   ├── pty.rs            # Local PTY connection
│   ├── ssh/              # SSH connection (auth, session, terminal adapter)
│   └── telnet/           # Telnet connection (protocol, session, terminal adapter)
├── session_store.rs      # Session persistence in tree structure
├── session_logger.rs     # Terminal output logging
├── active_session_tracker.rs  # GPUI entity for active sessions
├── rule_store.rs         # Automation rule data model (uses JsonConfigStore)
├── rule_engine.rs        # Rule execution with regex matching
├── recognize_config.rs   # Quick Add auto-recognition rules (version-aware defaults)
├── abbr_store.rs         # Command abbreviations (uses JsonConfigStore)
├── function_store.rs     # Function bar configurations (uses JsonConfigStore)
├── highlight_rule.rs     # Highlight rule types and TerminalTokenType
├── highlight_store.rs    # Highlight rule storage (uses JsonConfigStore)
├── button_bar_config.rs  # Button bar configuration (uses JsonConfigStore)
├── shortcut_bar_store.rs # Keyboard/script shortcuts (custom keybinding logic)
├── command_history.rs    # Command history tracking
├── terminal_settings.rs  # Terminal configuration
├── terminal_hyperlinks.rs # Hyperlink detection (102KB)
└── mappings/             # Input/output mappings
    ├── keys.rs           # Keyboard → escape sequences
    ├── colors.rs         # Color space conversions
    └── mouse.rs          # Mouse event handling
```

## Key Types

| Type | Purpose |
|------|---------|
| `Terminal` | Main terminal entity wrapping alacritty Term |
| `TerminalConnection` | Trait for connection backends (PTY, SSH, Telnet) |
| `ConnectionState` | Connected/Connecting/Disconnected/Error enum |
| `ConfigItem` / `JsonConfigStore` | Generic traits for JSON config stores (see config_store.rs) |
| `SessionStore` / `SessionStoreEntity` | Session persistence with GPUI integration |
| `SessionNode` | Group or Session in tree structure |
| `SessionConfig` | SSH/Telnet configuration with auth methods |
| `RuleStore` / `RuleEngine` | Automation rules with pattern matching |
| `AutomationRule` | Trigger + condition + action definition |
| `RecognizeConfig` / `RecognizeConfigEntity` | Quick Add auto-recognition rules (version-aware defaults) |
| `AbbreviationStore` | Command abbreviations with protocol filtering |
| `FunctionStore` | Function bar configurations with protocol filtering |
| `HighlightStore` | Highlight rules with priority sorting |
| `ButtonBarStore` | Button bar configurations |
| `Event` | Terminal events (title changed, disconnected, login complete) |

## Dependencies

- `alacritty_terminal` - Terminal emulation engine
- `gpui` - Entity management and async context
- `settings` - Terminal configuration
- `task` - Shell integration

## Common Tasks

**Add a new connection type:**
1. Create module in `connection/`
2. Implement `TerminalConnection` trait
3. Add variant to connection factory in `terminal.rs`

**Add automation rule action:**
1. Add variant to `RuleAction` in `rule_store.rs`
2. Implement execution in `rule_engine.rs`

**Add terminal event:**
1. Add variant to `Event` enum in `terminal.rs`
2. Emit via `cx.emit()` at appropriate location

**Add a new config store:**
1. Define item type implementing `ConfigItem` (needs `fn id(&self) -> Uuid`)
2. Define store struct implementing `JsonConfigStore` (provide `items()`, `items_mut()`, `new_empty()`)
3. Add domain-specific methods (protocol filtering, defaults, etc.)
4. Create `*StoreEntity` wrapper with Global marker, `EventEmitter`, and `schedule_save()`
5. Add path function in `crates/paths/src/paths.rs`
6. Use `default_true()` from `config_store` for serde defaults

## Testing

```sh
cargo test -p terminal
cargo test -p terminal rule_engine  # Rule engine tests
```

## Pitfalls

- `events_tx` must be preserved during reconnection to maintain scrollback history
- Rule engine has 2-second cooldown between trigger matches
- Protocol negotiation (Telnet IAC) must be handled before passing data to Term
- Session store auto-saves on changes - avoid unnecessary mutations
- `recognize_config.json` is only overwritten when its version is older than the app's embedded version
