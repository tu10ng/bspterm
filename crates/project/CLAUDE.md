# project

File management, navigation, and LSP communication. Coordinates worktrees, buffers, language servers, and git integration.

## Module Structure

```
src/
├── project.rs            # Core Project entity
├── worktree_store.rs     # Worktree management
├── buffer_store.rs       # Open buffer management
├── lsp_store.rs          # LSP lifecycle and operations (590KB)
├── git_store.rs          # Git integration (blame, status)
├── search.rs             # Text/regex search
├── project_search.rs     # Project-wide search
├── task_store.rs         # Task/build system
├── task_inventory.rs     # Task discovery
├── prettier_store.rs     # Code formatter integration
├── toolchain_store.rs    # Language toolchain management
├── environment.rs        # Project environment variables
├── terminals.rs          # Terminal session management
├── project_settings.rs   # Project-specific settings
├── image_store.rs        # Image asset management
├── manifest_tree.rs      # Package manifest parsing
├── debugger/             # Debug adapter protocol
├── lsp_store/            # LSP submodules
└── lsp_command/          # LSP command execution
```

## Key Types

| Type | Purpose |
|------|---------|
| `Project` | Main entity coordinating all project state |
| `Worktree` | Virtual file system container (local/remote) |
| `WorktreeStore` | Manages Worktree entities |
| `BufferStore` | Manages open Buffer entities |
| `LspStore` | LSP server lifecycle and operations |
| `GitStore` | Git operations (blame, status) |
| `ProjectPath` | Full path in project space |
| `ProjectEntryId` | Unique file/folder identifier |
| `WorktreeId` | Worktree identifier |
| `BufferId` | Buffer identifier |

## Dependencies

- `gpui` - Entity management
- `language` - Buffer, syntax, LSP types
- `lsp` - LSP protocol
- `worktree` - Filesystem access
- `git` - Git operations

## Common Tasks

**Open a file:**
```rust
let buffer = project.open_buffer(project_path, cx).await?;
```

**Register LSP capability:**
1. Add handler in `lsp_store.rs`
2. Wire to LSP request/notification

**Add project-wide search:**
1. Use `SearchQuery` in `search.rs`
2. Search returns `SearchResult::Buffer` with ranges

## Testing

```sh
cargo test -p project
```

## Pitfalls

- Project can be local (manages worktrees) or remote (proxies via RPC)
- `ProjectPath` format: `"worktree-id:path/to/file"`
- BufferStore and WorktreeStore have distinct local/remote implementations
- LSP servers loaded on-demand per language per worktree
- Diagnostics stored per-server in `DiagnosticSet`
- Worktrees emit events: `WorktreeUpdatedEntries`, `WorktreeAdded`
- BufferStore emits: `BufferAdded`, `BufferDropped`, `BufferPathChanged`
- LspStore emits status updates for language server lifecycle
