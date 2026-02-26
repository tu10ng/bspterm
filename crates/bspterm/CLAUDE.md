# bspterm

Main application entry point that ties everything together. Handles initialization, workspace setup, and application lifecycle.

## Module Structure

```
src/
├── main.rs              # Application entry point (1,730 lines)
├── bspterm.rs           # Core initialization (5,000+ lines)
├── reliability.rs       # Crash handling
├── visual_test_runner.rs # Visual tests runner
└── bspterm/
    ├── app_menus.rs     # Menu definitions (13KB)
    ├── migrate.rs       # Data migration (12KB)
    ├── open_listener.rs # IPC socket listener (41KB)
    ├── quick_action_bar.rs # Command palette (37KB)
    ├── telemetry_log.rs # Telemetry events (21KB)
    ├── visual_tests.rs  # Visual testing (19KB)
    ├── edit_prediction_registry.rs # AI predictions
    ├── mac_only_instance.rs  # macOS single-instance
    ├── windows_only_instance.rs # Windows single-instance
    └── open_url_modal.rs # URL entry dialog
```

## Key Functions

| Function | Purpose |
|----------|---------|
| `main()` | CLI parsing, path init, app creation |
| `bspterm::init(cx)` | Register global actions |
| `initialize_workspace()` | Set up panels, status bar, watchers |
| `initialize_panels()` | Create dock panels |
| `register_actions()` | Register workspace actions |

## Startup Flow

```
main()
  ├─ Parse CLI arguments (clap)
  ├─ Initialize paths (config, extensions, logs)
  ├─ Set up logging (zlog, ztracing)
  ├─ Check single-instance lock (platform-specific)
  ├─ Create GPUI Application
  ├─ app.run(|cx| {
  │    ├─ Initialize core modules (themes, settings, languages)
  │    ├─ Create AppState (languages, client, fs)
  │    ├─ bspterm::init(cx)
  │    ├─ Observe new workspaces → initialize_workspace()
  │    └─ Load initial workspace or files
  │  })
  └─ Listen for open requests via OpenListener
```

## Dependencies

- `gpui` - Application framework
- `workspace` - Window management
- `terminal_view` - Terminal UI
- `remote_explorer` - Session panel
- `settings` - Configuration
- Many more (170+ workspace crates)

## Common Tasks

**Add a global action:**
1. Define action in appropriate crate
2. Register in `bspterm::init()` using `cx.on_action()`

**Add a status bar item:**
1. Create component implementing status bar item trait
2. Register in `initialize_workspace()`

**Add a panel:**
1. Implement Panel trait in panel crate
2. Register in `initialize_panels()`

## Testing

```sh
cargo test -p bspterm
cargo run -p cli  # Run release CLI
```

## Pitfalls

- Single-instance handling differs by platform:
  - Linux: Socket listener via `listen_for_cli_connections()`
  - Windows: `windows_only_instance::handle_single_instance()`
  - macOS: `mac_only_instance::ensure_only_instance()`
- Special CLI modes: `--askpass`, `--crash-handler`, `--nc`, `--printenv`
- Theme loading is eager (load active theme at startup)
- File watchers: themes directory, languages directory (debug), user settings
- Panels registered: RemoteExplorer, RuleEditor, EditorPanel, TerminalPanel, ProjectPanel, OutlinePanel
- Global entities initialized: SessionStore, AbbreviationStore, RuleStore, ShortcutBarStore
