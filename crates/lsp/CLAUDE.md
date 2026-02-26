# lsp

LSP client implementation for communicating with language servers. Handles JSON-RPC protocol and server lifecycle.

## Module Structure

```
src/
├── lsp.rs           # Main implementation (84KB)
└── input_handler.rs # Stdout message parsing (5KB)
```

## Key Types

| Type | Purpose |
|------|---------|
| `LanguageServer` | Running LSP server process |
| `LanguageServerId` | Numeric server identifier |
| `LanguageServerName` | String server identifier |
| `LanguageServerBinary` | Binary path, args, environment |
| `Subscription` | Notification/request handler subscription |
| `RequestId` | Int or String request ID |
| `Request<T>` / `Response<T>` | Typed JSON-RPC messages |
| `Notification<T>` | JSON-RPC notification |
| `Error` | JSON-RPC error with code |
| `AdapterServerCapabilities` | Server capabilities wrapper |

## Dependencies

- `lsp-types` - LSP protocol types (re-exported)
- `serde_json` - JSON serialization
- `smol` - Async runtime
- `futures` - Async utilities

## Common Tasks

**Send a request:**
```rust
let result = server.request::<lsp::GotoDefinition>(params, timeout).await?;
```

**Send a notification:**
```rust
server.notify::<lsp::DidOpenTextDocument>(params)?;
```

**Subscribe to notifications:**
```rust
let subscription = server.on_notification::<lsp::PublishDiagnostics>(|params, cx| {
    // Handle diagnostics
});
```

**Initialize server:**
```rust
let server = LanguageServer::new(binary, root_uri, cx)?;
let server = server.initialize(params, cx).await?;
```

## Testing

```sh
cargo test -p lsp
```

## Pitfalls

- JSON-RPC 2.0 with content-length based message framing
- `LspStdoutHandler` parses Content-Length headers
- Requests have configurable timeout via `Duration`
- `Subscription` dropped → handler unregistered
- Server lifecycle: `new()` → `initialize()` → active → `shutdown()`
- Semantic token types and modifiers are predefined constants
- Dynamic workspace folder support via `add_workspace_folder()`
- `on_io()` subscription allows inspecting raw stdin/stdout/stderr
