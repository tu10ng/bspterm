# terminal_outline

Panel displaying terminal command history with timestamps for navigation.

## Module Structure

```
src/
└── terminal_outline.rs  # Single-file crate (library root)
```

## Key Types

| Type | Purpose |
|------|---------|
| `TerminalOutline` | Main panel (Panel, Render, Focusable) |
| `OutlineEntry` | Command entry in outline list |

## Dependencies

- `terminal` - Terminal entity, command history
- `terminal_view` - TerminalView detection
- `workspace` - Panel, workspace events
- `gpui` - UI framework
- `ui` - Label, Icon components
- `i18n` - Internationalization

## Common Tasks

**Add outline entry metadata:**
1. Update `OutlineEntry` struct
2. Modify entry rendering in `render()`

**Add click behavior:**
1. Update click handler to scroll terminal to command

## Testing

```sh
cargo test -p terminal_outline
```

## Pitfalls

- Panel tracks active workspace item to detect terminal changes
- Subscribes to `TerminalEvent::CommandHistoryChanged` for updates
- Uses UniformList for efficient virtual scrolling
- Entry click scrolls terminal to that command's line
- Default position: Right dock (also allows Left)
- Default width: 280px
- Activation priority: 15
- Timestamp format: `HH:MM:SS`
- Prompt text highlighted in accent color
