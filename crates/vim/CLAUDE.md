# vim

Vim mode implementation over the editor. Provides modal editing with motions, operators, and text objects.

## Module Structure

```
src/
├── vim.rs            # Main Vim entity (82KB)
├── state.rs          # Core state management (64KB)
├── motion.rs         # Motion definitions (175KB, largest)
├── command.rs        # Command parsing (123KB)
├── normal.rs         # Normal mode operations (78KB)
├── visual.rs         # Visual mode operations (72KB)
├── object.rs         # Text objects (133KB)
├── helix.rs          # Helix editor compatibility (63KB)
├── surrounds.rs      # Surround text manipulation (56KB)
├── insert.rs         # Insert mode (7KB)
├── replace.rs        # Replace mode (20KB)
├── indent.rs         # Indentation operations
├── change_list.rs    # Change history
├── digraph.rs        # Digraph characters
├── mode_indicator.rs # Mode display UI
├── normal/           # Normal mode sub-operations
│   ├── change.rs, delete.rs, yank.rs
│   ├── mark.rs, paste.rs, repeat.rs
│   ├── search.rs, substitute.rs
│   └── ...
├── helix/            # Helix compatibility
└── test/             # Testing infrastructure
```

## Key Types

| Type | Purpose |
|------|---------|
| `Vim` | Main entity managing vim state |
| `Mode` | Normal/Insert/Replace/Visual/VisualLine/VisualBlock/HelixNormal/HelixSelect |
| `Operator` | Change/Delete/Yank/Replace/Object/Find/Sneak |
| `Motion` | Cursor movement definitions (50+ variants) |
| `MotionKind` | Linewise/Exclusive/Inclusive |

## Key Motions

- **Basic:** Left, Right, Up, Down, WrappingLeft, WrappingRight
- **Word:** NextWordStart, NextWordEnd, PreviousWordStart
- **Line:** FirstNonWhitespace, StartOfLine, EndOfLine
- **Document:** StartOfDocument, EndOfDocument, GoToPercentage
- **Find:** FindForward, FindBackward, Sneak, RepeatFind
- **Structural:** NextMethodStart, UnmatchedForward

## Dependencies

- `gpui` - Entity management, rendering
- `editor` - Core editor component
- `language` - Syntax information
- `search` - Buffer search
- `settings` - VimModeSetting, HelixModeSetting

## Common Tasks

**Add a motion:**
1. Add variant to `Motion` enum in `motion.rs`
2. Implement motion logic in `motion()` function
3. Add keybinding

**Add an operator:**
1. Add variant to `Operator` enum in `state.rs`
2. Implement in appropriate mode file

**Add text object:**
1. Add to `object.rs`
2. Wire to `a` and `i` commands

## Testing

```sh
cargo test -p vim
cargo test -p vim --features neovim  # Test against real Neovim
```

## Pitfalls

- Mode tracking via `Mode` enum with transitions
- Operator stacking for vim-style commands (e.g., `dw`, `c3w`)
- Count system via `Number` action (e.g., `3dw`)
- Register selection for named registers
- Helix compatibility mode provides parallel mode system
- `Replayer` handles macro recording and command repeat
- Keystroke parsing happens before mode-specific handlers
- `mode_indicator` renders current mode in status bar
