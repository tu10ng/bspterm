# remote_connection

UI components for remote connection dialogs and credential prompts. Bridges GPUI async context with remote connection lifecycle.

## Module Structure

```
src/
└── remote_connection.rs  # Single-file crate (library root)
```

## Key Types

| Type | Purpose |
|------|---------|
| `RemoteConnectionPrompt` | Password/credential input UI entity |
| `RemoteConnectionModal` | Modal wrapper for connection prompts |
| `RemoteClientDelegate` | Implements remote::RemoteClientDelegate trait |
| `SshConnectionHeader` | UI component showing connection metadata |

## Dependencies

- `remote` - RemoteClientDelegate trait
- `gpui` - UI framework, async context
- `workspace` - ModalView integration
- `ui` - Shared UI components
- `editor` - Password input editor
- `askpass` - Credential handling
- `auto_update` - Server binary downloads

## Common Tasks

**Customize connection prompt:**
1. Update `RemoteConnectionPrompt` in `remote_connection.rs`
2. Modify `render()` implementation

**Add connection metadata display:**
1. Update `SshConnectionHeader` struct
2. Add fields to render output

## Testing

```sh
cargo test -p remote_connection
```

## Pitfalls

- `RemoteConnectionPrompt` manages editor state for password input with masking
- Modal uses `EventEmitter<DismissEvent>` for proper lifecycle management
- `RemoteClientDelegate` bridges async password prompts via oneshot channels
- `connect()` function supports cancellation via oneshot channels
- Connection header displays: host, nickname, paths, connection type icon
