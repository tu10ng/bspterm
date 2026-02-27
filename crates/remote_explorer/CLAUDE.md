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
    ├── auto_recognize.rs   # Flexible IP/connection parser with smart token classification
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
| `TokenType` | Ip/Port/Username/Password/Label classification |
| `DraggedSessionEntry` | Drag-drop data |
| `DragTarget` | Drop target indicator enum |

## Auto-Recognize Parser

The parser in `auto_recognize.rs` uses heuristic token classification:

### Token Classification Rules

| Type | Detection Logic |
|------|-----------------|
| IP | Valid IPv4 address (e.g., `192.168.1.1`) |
| Port | Pure digits 1-65535 |
| Username | Common names (root, admin, huawei, cisco) or starts with them |
| Password | Contains `@!#$%^&*` or has mixed case + digits |
| Label | Non-ASCII chars (中文) or alphanumeric mix (slot23) |

### Supported Formats

| Format | Example | Result |
|--------|---------|--------|
| IP only | `192.168.1.1` | Telnet:23 |
| IP:port | `192.168.1.1:22` | SSH:22 |
| Smart credentials | `192.168.1.1 root123 Root@123` | user=root123, pass=Root@123 |
| With label | `192.168.1.1 root Root@123 slot23` | Name includes "slot23" |
| Chinese prefix | `管理网口192.168.1.1 huawei Admin@123` | Name=管理网口..., with auth |
| Multi-line | `6.6.62.23 slot23\nhuawei\nRouter@202508` | Single connection |
| Bulk import | `环境192.168.1.1\troot\tpass` | Telnet, grouped by IP |

### Session Grouping

All quick-add sessions are automatically placed in a group named after the IP address. If the group doesn't exist, it's created automatically.

## Dependencies

- `terminal` - SessionStore, SessionConfig, AuthMethod
- `workspace` - Panel, ModalView, DockPosition
- `editor` - Text input editors
- `ui` - ListItem, ContextMenu, Button, Icon
- `lan_discovery` - Online user detection

## Common Tasks

**Add a new quick add input format:**
1. Update parser in `auto_recognize.rs`
2. Update `classify_token()` if new token type needed
3. Add test cases for new format

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
- Auto-recognize uses heuristics - passwords with only alphanumeric chars may be misclassified as usernames
- Multi-line detection requires IP only on first line; if multiple lines have IPs, treated as separate connections
