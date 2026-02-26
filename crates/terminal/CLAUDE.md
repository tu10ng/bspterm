# terminal

Core terminal emulation engine providing terminal rendering, SSH/Telnet connections, automation rules, and session management. Built on top of alacritty_terminal.

## Module Structure

```
src/
├── terminal.rs           # Main Terminal entity (184KB)
├── connection/           # Terminal connection backends
│   ├── mod.rs            # TerminalConnection trait, ConnectionState
│   ├── pty.rs            # Local PTY connection
│   ├── ssh/              # SSH connection (auth, session, terminal adapter)
│   └── telnet/           # Telnet connection (protocol, session, terminal adapter)
├── session_store.rs      # Session persistence in tree structure
├── session_logger.rs     # Terminal output logging
├── active_session_tracker.rs  # GPUI entity for active sessions
├── rule_store.rs         # Automation rule data model
├── rule_engine.rs        # Rule execution with regex matching
├── abbr_store.rs         # Command abbreviations with protocol filtering
├── button_bar_config.rs  # Button bar configuration
├── shortcut_bar_store.rs # Keyboard/script shortcuts
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
| `SessionStore` / `SessionStoreEntity` | Session persistence with GPUI integration |
| `SessionNode` | Group or Session in tree structure |
| `SessionConfig` | SSH/Telnet configuration with auth methods |
| `RuleStore` / `RuleEngine` | Automation rules with pattern matching |
| `AutomationRule` | Trigger + condition + action definition |
| `AbbreviationStore` | Command abbreviations with protocol filtering |
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
