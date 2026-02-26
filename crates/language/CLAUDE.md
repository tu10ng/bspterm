# language

Language understanding including syntax trees, buffers, and symbol extraction. Provides Tree-sitter integration and LSP adapter infrastructure.

## Module Structure

```
src/
├── language.rs            # Library root, Language types
├── buffer.rs              # Core Buffer entity (5,600+ lines)
├── language_registry.rs   # Language discovery and loading
├── syntax_map.rs          # Multi-layer syntax trees (2,000+ lines)
├── diagnostic_set.rs      # LSP diagnostic storage
├── outline.rs             # Symbol extraction
├── highlight_map.rs       # Syntax highlight mapping
├── language_settings.rs   # Per-language configuration
├── text_diff.rs           # Diff computation
├── task_context.rs        # Task/runnable detection
├── toolchain.rs           # Language toolchain management
└── manifest.rs            # Package manifest handling
```

## Key Types

| Type | Purpose |
|------|---------|
| `Language` | Language configuration and grammar |
| `LanguageConfig` | Declarative language behavior config |
| `LanguageRegistry` | Global registry for language loading |
| `Grammar` | Tree-sitter grammar with queries |
| `Buffer` | In-memory source file (Entity<Buffer>) |
| `BufferSnapshot` | Immutable buffer state snapshot |
| `SyntaxMap` | Multi-layer syntax tree manager |
| `SyntaxSnapshot` | Immutable syntax trees snapshot |
| `DiagnosticSet` | LSP diagnostics (SumTree storage) |
| `Outline<T>` | Symbol list for navigation |
| `OutlineItem<T>` | Single symbol with range |
| `CachedLspAdapter` | LSP adapter with caching |

## Dependencies

- `tree-sitter` (via WASM) - Parsing and querying
- `text` - Text buffer primitives (Anchor, Point, Rope)
- `gpui` - Entity management
- `lsp` - LSP types
- `settings` - Configuration

## Common Tasks

**Add language support:**
1. Create grammar WASM and queries
2. Add `LanguageConfig` in languages directory
3. Register with `LanguageRegistry`

**Add syntax query:**
1. Add query file (highlights, indents, outline)
2. Reference in `Grammar` configuration

**Access buffer diagnostics:**
```rust
let diagnostics = buffer.diagnostics_for_range(range, cx);
```

## Testing

```sh
cargo test -p language
cargo test -p language buffer_tests
```

## Pitfalls

- Tree-sitter grammars loaded via WASM for safety
- `SyntaxMap` supports embedded languages (injection)
- Buffer tracks per-server `DiagnosticSet` instances
- `BufferSnapshot` is cheap to clone (structural sharing)
- Parser and query cursor pools reuse expensive objects
- Language registry version tracking enables cache invalidation
- Outline extraction uses Tree-sitter capture queries
- `LanguageConfig` supports 100+ properties (brackets, comments, etc.)
