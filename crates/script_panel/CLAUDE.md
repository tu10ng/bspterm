# script_panel

Panel for Python script discovery, execution, and output display. Integrates with terminal_scripting for automation.

## Module Structure

```
src/
├── script_panel.rs   # Main panel UI and initialization
└── script_runner.rs  # Cross-platform Python execution
```

## Key Types

| Type | Purpose |
|------|---------|
| `ScriptPanel` | Main panel (Panel, Focusable, Render) |
| `ScriptRunner` | Cross-platform Python executor |
| `ScriptStatus` | NotStarted/Running/Finished(i32)/Failed(String) |
| `ScriptEntry` | Script metadata (name, path) |

## Dependencies

- `terminal_scripting` - ScriptingServer, terminal registry
- `workspace` - Panel framework
- `gpui` - UI primitives
- `ui` - Shared UI components

## Common Tasks

**Add a default script:**
1. Add script to `assets/scripts/`
2. Update installation logic in `script_panel.rs`

**Add script execution option:**
1. Update `ScriptRunner::start()` in `script_runner.rs`
2. Pass new environment variables if needed

## Testing

```sh
cargo test -p script_panel
```

## Pitfalls

- Scripts directory: `~/.config/bspterm/scripts/`
- `bspterm.py` is auto-installed but excluded from script list
- Environment variables passed to scripts:
  - `BSPTERM_SOCKET` - Unix socket connection string
  - `PYTHONPATH` - Points to scripts directory
  - `BSPTERM_CURRENT_TERMINAL` - Focused terminal UUID
- Cross-platform I/O:
  - Unix: Uses `fcntl()` for non-blocking I/O
  - Windows: Uses `PeekNamedPipe()` for non-blocking reads
  - Windows: `CREATE_NO_WINDOW` flag hides console
- Panel docks on Bottom by default (priority: 20)
