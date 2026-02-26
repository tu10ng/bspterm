# editor

Core text editor component used for code editing and input fields. Handles LSP display features, multi-selection, and code intelligence.

## Module Structure

```
src/
├── editor.rs              # Main Editor entity (1.08 MB)
├── element.rs             # GPUI rendering (540 KB)
├── display_map.rs         # Text transformation pipeline (141 KB)
├── display_map/           # Transformation layers
│   ├── inlay_map.rs       # Inlay hints injection
│   ├── fold_map.rs        # Code folding
│   ├── tab_map.rs         # Hard tab → spaces
│   ├── wrap_map.rs        # Soft text wrapping
│   ├── block_map.rs       # Custom blocks (diagnostics)
│   └── crease_map.rs      # Editor creases/rulers
├── selections_collection.rs  # Multi-selection state
├── scroll.rs              # Scrolling logic
├── actions.rs             # Action definitions
├── items.rs               # Workspace item integration
├── code_context_menus.rs  # Completion, code actions
├── hover_popover.rs       # Hover information
├── hover_links.rs         # Go-to-definition
├── signature_help.rs      # Function signatures
├── inlays.rs              # Inlay hints
├── git/                   # Git integration (blame)
└── test/                  # Testing utilities
```

## Key Types

| Type | Purpose |
|------|---------|
| `Editor` | Core editor entity (wraps MultiBuffer) |
| `EditorElement` | GPUI rendering container |
| `EditorSnapshot` | Immutable snapshot for rendering |
| `DisplayMap` | Text transformation pipeline |
| `SelectionsCollection` | Multi-selection management |
| `DisplayPoint` | Rendered coordinate (after wraps, folds) |
| `Anchor` | Position-independent buffer reference |
| `Selection<Anchor>` | Individual selection with anchors |

## Display Transformation Layers

The `DisplayMap` uses a multi-layer architecture:
1. **InlayMap** - Injects inlay hints
2. **FoldMap** - Handles code folding
3. **TabMap** - Converts hard tabs to spaces
4. **WrapMap** - Applies soft text wrapping
5. **BlockMap** - Inserts custom blocks
6. **CreaseMap** - Adds editor creases

## Dependencies

- `gpui` - UI framework
- `language` - Buffer, syntax trees
- `project` - LSP integration
- `multi_buffer` - Multi-file editing
- `theme` - Syntax highlighting

## Common Tasks

**Add editor action:**
1. Define action in `actions.rs`
2. Implement handler method on `Editor`
3. Register with `.on_action()`

**Add LSP feature:**
1. Implement handler in appropriate module (hover, completion, etc.)
2. Wire to LSP request in `project` crate

**Add gutter feature:**
1. Update layout in `element.rs`
2. Add paint logic for new gutter element

## Testing

```sh
cargo test -p editor
cargo test -p editor editor_tests  # Main test suite
```

## Pitfalls

- `EditorSnapshot` is immutable - take new snapshot after mutations
- Display coordinates differ from buffer coordinates (due to folding, wrapping)
- Each transformation layer provides coordinate conversion functions
- Multi-selection: `SelectionsCollection` with pending selections
- Semantic tokens from LSP override tree-sitter highlighting
- Inlay hints are virtual text injected by InlayMap
- `cx.notify()` required after state changes for re-render
