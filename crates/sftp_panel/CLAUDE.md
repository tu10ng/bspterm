# sftp_panel

Panel for browsing remote file systems over SFTP. Connects to SSH hosts using credentials from active terminal sessions.

## Module Structure

```
src/
└── sftp_panel.rs    # Panel UI, connection logic, file browser rendering
```

## Key Types

| Type | Purpose |
|------|---------|
| `SftpPanel` | Main panel (Panel, Focusable, Render, PanelHeader) |
| `ConnectionStatus` | Disconnected/Connecting/Connected/Error state |
| `FileEntry` | Tree entry for the file list (RemoteEntry + depth + parent_path) |
| `SortMode` | Name or ModifiedTime sorting |
| `EditState` | Inline editor state for rename/new file/new directory |
| `ClipboardState` | Internal clipboard for cut/copy/paste operations |
| `DraggedSftpEntry` | Drag data for internal drag-and-drop (path, name, is_dir) |
| `SftpDragTarget` | Drop target indicator (IntoDir, BeforeEntry, AfterEntry) |

## Actions

All actions are defined in `bspterm_actions::sftp_panel`:

| Action | Description |
|--------|-------------|
| `SelectNext` / `SelectPrevious` | Keyboard navigation through entries |
| `Confirm` | Toggle directory expansion or open file |
| `GoUp` | Navigate to parent directory |
| `Cancel` | Dismiss edit/filter state, clear selection |
| `Open` | Download file to temp and open in editor; navigate into directory |
| `NewFile` / `NewDirectory` | Create entry via inline editor |
| `Rename` | Rename entry via inline editor |
| `Delete` | Delete with recursive support for directories |
| `Cut` / `Copy` / `Paste` | Internal clipboard (cut uses rename, copy uses download+upload) |
| `CopyPath` | Copy remote path to system clipboard |
| `Download` | Save selected entries to `~/Downloads` |
| `Chmod` | Toggle executable bit on selected entry |
| `ToggleSortMode` | Cycle between Name and ModifiedTime sort |
| `ToggleFilter` | Show/hide inline filter editor |
| `EditPath` | Click path bar to edit, Enter navigates to path |
| `RefreshDirectory` | Sync CWD from terminal and reload |

## Dependencies

- `terminal` - SftpStore, SftpClient, SshConfig, ConnectionInfo
- `terminal_view` - TerminalView downcast for extracting SSH credentials
- `workspace` - Panel framework, open_abs_path for file viewing
- `gpui` - UI primitives, entity management
- `ui` - Shared UI components (ListItem, IconButton, Label, ContextMenu)
- `panel` - PanelHeader trait
- `editor` - Inline editors for rename/new/filter

## Common Tasks

**Connect to a host programmatically:**
```rust
sftp_panel.update(cx, |panel, cx| {
    let config = SshConfig::new("host", 22)
        .with_username("user")
        .with_auth(SshAuthConfig::Password("pass".into()));
    panel.connect(config, cx);
});
```

**Connect from active terminal:**
The panel extracts SSH credentials from the focused terminal's `ConnectionInfo::Ssh` variant and initiates an SFTP connection automatically.

## Testing

```sh
cargo test -p sftp_panel
```

## Pitfalls

- Panel docks on Right by default (priority: 3), can be moved to Left
- Uses `uniform_list` for virtual scrolling — file list is a tree with entries at varying depths
- Single-click on a directory toggles inline expansion (children loaded as siblings); `Open` action navigates into the directory (changes root)
- `expanded_dirs: HashSet<String>` tracks which directories have their children visible in the tree
- Drag-and-drop: internal reorder via `DraggedSftpEntry`, hover over collapsed dir for 500ms auto-expands it
- Tree-aware sorting preserves sibling groups; directories always sort before files at each depth level
- `SftpStoreEntity` must be initialized via `sftp_panel::init(cx)` before use
- Connection reuses existing `SftpClient` if one exists for the same host+port (via `SftpStore::get_or_connect`)
- `connect_from_active_terminal` only works with SSH terminals, not Telnet
- `follow_active_terminal()` uses `followed_terminal_id` to skip redundant syncs when the same terminal fires `ActiveItemChanged` (e.g. on every keypress via Wakeup → ChangeItemTitle). Only actual tab switches trigger `sync_from_terminal()` / `connect()`.
- File sizes formatted with `format_file_size()` (B/KB/MB/GB)
- Navigate-up from "/" is a no-op; navigating up or into a directory clears expanded state
- Sorting always places directories before files at each depth level
- Watch/refresh re-expands previously expanded directories automatically
- `open_file` downloads to a temp dir (`/tmp/bspterm_sftp/<host>/`) before opening
- Paste of copied files uses download-then-upload (no server-side copy)
- Context menu appears on right-click, adapts to selection state
- Path bar is clickable — click to edit, Enter to navigate (supports `~` via `realpath()`), Escape/blur to cancel. File paths navigate to parent directory.
