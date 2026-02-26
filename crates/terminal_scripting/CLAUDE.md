# terminal_scripting

Python automation API via Unix socket/TCP JSON-RPC server. Provides programmatic access to terminal operations.

## Module Structure

```
src/
├── terminal_scripting.rs  # Module entry and exports
├── server.rs              # Unix socket/TCP server
├── protocol.rs            # JSON-RPC 2.0 types
├── handlers.rs            # 22 JSON-RPC method handlers
├── session.rs             # Terminal registry management
└── tracking.rs            # Output/command tracking
```

## Key Types

| Type | Purpose |
|------|---------|
| `ScriptingServer` | Platform-aware JSON-RPC server |
| `JsonRpcRequest` / `JsonRpcResponse` | JSON-RPC 2.0 messages |
| `TerminalRegistry` | Static registry for active terminals |
| `TerminalSession` | Terminal wrapper with metadata |
| `OutputTracker` | Timestamped output buffering |
| `ReaderState` / `ReaderId` | Output reader position tracking |
| `CommandExecution` | Command lifecycle tracking |
| `SessionInfo` | Terminal session metadata |
| `ScreenContent` | Terminal screen state |

## JSON-RPC Methods

**Session Methods:**
- `session.current` - Get focused terminal info
- `session.list` - List all registered terminals
- `session.create_ssh` / `session.create_telnet` - Create terminal
- `session.add_ssh_to_group` - Add session to folder

**Terminal Methods:**
- `terminal.send` - Send raw data
- `terminal.read` - Read screen content
- `terminal.wait_for` - Wait for regex pattern (30s timeout)
- `terminal.wait_for_login` - Wait for login completion
- `terminal.run` - Send command and wait for prompt
- `terminal.sendcmd` - Send with optional echo stripping
- `terminal.close` - Close connection

**Tracking Methods:**
- `terminal.track_start` / `terminal.track_read` / `terminal.track_stop`
- `terminal.run_marked` - Run with command ID
- `terminal.read_command_output` - Get command output
- `terminal.read_time_range` - Read output in time window

**UI Methods:**
- `pane.split_right_clone` - Split pane and clone
- `notify.toast` - Show toast notification

## Dependencies

- `terminal` - Terminal entity access
- `gpui` - Async context
- `serde_json` - JSON serialization
- Platform-specific: Unix sockets or TCP

## Common Tasks

**Add a JSON-RPC method:**
1. Add parameter struct in `protocol.rs`
2. Add handler in `handlers.rs`
3. Register method in `handle_request()`

**Add terminal event notification:**
1. Create notification type in `protocol.rs`
2. Emit from appropriate location in terminal crate

## Testing

```sh
cargo test -p terminal_scripting
```

## Pitfalls

- **Platform-aware transport:**
  - Linux/macOS: Unix socket at `$XDG_RUNTIME_DIR/bspterm-{pid}.sock`
  - Windows: TCP localhost on auto-assigned port
- OutputTracker has limits: 10MB max, 1000 segments max
- `terminal.wait_for` default timeout is 30 seconds
- `terminal.run` default prompt regex is `[$#>]\s*$`
- Multiple independent readers can track output simultaneously
