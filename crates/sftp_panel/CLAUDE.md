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
| `FileEntry` | Flattened entry for the file list (RemoteEntry + depth) |

## Dependencies

- `terminal` - SftpStore, SftpClient, SshConfig, ConnectionInfo
- `terminal_view` - TerminalView downcast for extracting SSH credentials
- `workspace` - Panel framework
- `gpui` - UI primitives, entity management
- `ui` - Shared UI components (ListItem, IconButton, Label)
- `panel` - PanelHeader trait

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
- Uses `uniform_list` for virtual scrolling — file list must be flattened
- `SftpStoreEntity` must be initialized via `sftp_panel::init(cx)` before use
- Connection reuses existing `SftpClient` if one exists for the same host+port (via `SftpStore::get_or_connect`)
- `connect_from_active_terminal` only works with SSH terminals, not Telnet
- File sizes formatted with `format_file_size()` (B/KB/MB/GB)
- Navigate-up from "/" is a no-op
