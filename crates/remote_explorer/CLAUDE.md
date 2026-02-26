# remote_explorer

Panel for browsing and managing saved SSH/Telnet sessions in a tree view with drag-drop, quick add, and LAN discovery integration.

## Module Structure

```
src/
├── remote_explorer.rs      # Main panel (1,458 lines)
├── group_edit_modal.rs     # Group create/rename modal
├── session_edit_modal.rs   # Session configuration modal
└── quick_add/
    ├── mod.rs              # Quick Add area orchestration
    ├── auto_recognize.rs   # Flexible IP/connection parser
    ├── telnet_section.rs   # Telnet quick connect form
    ├── ssh_section.rs      # SSH quick connect form
    └── multi_connection_modal.rs  # Multi-selection modal
```

## Key Types

| Type | Purpose |
|------|---------|
| `RemoteExplorer` | Main panel (Panel, Focusable, Render) |
| `FlattenedEntry` | Flattened tree node for UniformList |
| `PingStatus` | Unknown/Checking/Reachable/Unreachable enum |
| `QuickAddArea` | Container for auto-recognize and forms |
| `GroupEditModal` | Modal for group management |
| `SessionEditModal` | Modal for session editing |
| `AutoRecognizeSection` | Flexible connection string parser |
| `DraggedSessionEntry` | Drag-drop data |
| `DragTarget` | Drop target indicator enum |

## Dependencies

- `terminal` - SessionStore, SessionConfig, AuthMethod
- `workspace` - Panel, ModalView, DockPosition
- `editor` - Text input editors
- `ui` - ListItem, ContextMenu, Button, Icon
- `lan_discovery` - Online user detection

## Common Tasks

**Add a new quick add input format:**
1. Update parser in `auto_recognize.rs`
2. Add test cases for new format

**Add context menu action:**
1. Add handler in `remote_explorer.rs`
2. Register in `render_context_menu()`

**Add session metadata field:**
1. Update `SessionConfig` in terminal crate
2. Add field to `SessionEditModal`

## Testing

```sh
cargo test -p remote_explorer
```

## Pitfalls

- Panel uses UniformList for virtual scrolling - tree must be flattened
- Indentation uses `ListItem.indent_level()` with `px(12.)` step size
- Ping refresh loop runs every 5 seconds - be mindful of network load
- Icons: `FolderOpen`/`Folder` for groups, `Server` for sessions
- Auto-recognize supports multiple formats (IP only, IP:port, with credentials, bulk)
