# terminal_view

UI layer for terminal emulation providing rendering, input handling, and session management panels. Implements the visual representation of terminals in the workspace.

## Module Structure

```
src/
├── terminal_view.rs        # Main TerminalView entity (implements Item, Render)
├── terminal_element.rs     # Low-level GPUI element for terminal grid
├── terminal_panel.rs       # Dockable panel for terminal panes
├── function_bar.rs         # Function bar modals (AddFunctionModal, EditAbbreviationModal, etc.)
├── button_bar.rs           # Custom button bar with script execution (@params modal support)
├── shortcut_bar.rs         # System action shortcuts panel
├── ssh_connect_modal.rs    # SSH connection setup dialog
├── terminal_scrollbar.rs   # Scrollbar state management
├── terminal_path_like_target.rs  # Path detection (Ctrl+Click to open)
├── terminal_slash_command.rs     # Slash command for Claude integration
└── persistence.rs          # Terminal session/pane serialization
```

## Key Types

| Type | Purpose |
|------|---------|
| `TerminalView` | Main terminal view entity (Item, Render, Focusable) |
| `TerminalElement` | GPUI Element for rendering terminal grid |
| `TerminalPanel` | Dockable panel containing terminal panes |
| `GroupKey` | Enum identifying tab groups (SessionGroup, Ungrouped, Local, Other) |
| `TerminalTabGroup` | A group of tabs with key, name, and indices |
| `TerminalGutterDimensions` | Gutter sizing (line numbers, timestamps) |
| `LayoutState` | Cached layout data for rendering |
| `TerminalInputHandler` | IME and keyboard input handler |
| `SshConnectModal` | SSH connection dialog |
| `SendText` / `SendKeystroke` | Actions to send input to terminal |
| `HighlightWord` / `ClearWordHighlights` | Actions for persistent word highlighting |
| `AddFunctionModal` | Modal for adding new function (script or abbreviation) |
| `EditAbbreviationModal` | Modal for editing abbreviation trigger/expansion/protocol |

## Dependencies

- `terminal` - Core terminal emulation
- `gpui` - UI framework
- `workspace` - Panel/item integration
- `editor` - Text editors for forms
- `ui` - Shared UI components
- `theme` - Visual styling

## Common Tasks

**Add a terminal action:**
1. Define action in `terminal_view.rs` using `actions!` macro
2. Register handler with `.on_action()`
3. Add keybinding in keymap

**Add gutter feature:**
1. Update `layout_gutter()` in `terminal_element.rs`
2. Add paint logic in `paint_gutter()`
3. Add setting in `terminal_settings.rs`

**Add toolbar button:**
1. Add to `button_bar.rs` or `shortcut_bar.rs`
2. Implement click handler

## Tab LED Indicator

SSH/Telnet terminal tabs display a colored LED indicator showing connection status:

| Color | Condition | Field |
|-------|-----------|-------|
| Green (`Color::Success`) | Connected, no new output | Default state |
| Blue (`Color::Info`) | New output while unfocused | `has_new_output` |
| Red (`Color::Error`) | Disconnected | `terminal.is_disconnected()` |

**Implementation** (`terminal_view.rs`):
- `has_new_output`: Set to `true` in `Event::Wakeup` handler when terminal is not focused
- Cleared in `focus_in()` when user switches back to the terminal tab
- `tab_led_color()`: Returns LED color based on priority (disconnected > new_output > connected)

Local terminals (no `connection_info`) do not display an LED.

## Grouped Tab Bar

When `group_tabs_by_session` setting is enabled, tabs are grouped by session in both dock and center panes. Key details:

- `TerminalPanel.group_overrides` maps non-terminal items (e.g., exported buffers) to a `GroupKey`
- `apply_grouped_tab_bar_to_center_panes()` sets the grouped renderer on workspace center panes
- `render_grouped_tab_bar()` renders group rows with drag-and-drop support between groups
- `TerminalView::group_key(cx)` returns the group key for a terminal
- Exported buffers inherit the source terminal's group via `register_item_group()`
- Falls back to `Pane::render_tab_bar` when no groups exist

## Testing

```sh
cargo test -p terminal_view
```

## Pitfalls

- Keybinding interception: `send_keybindings_to_shell` setting controls whether keystrokes go to shell or Bspterm actions
- Only keybindings with "Terminal" context predicate are intercepted
- `SendKeystroke` and `SendText` actions always go to shell even with Terminal context
- `on_drag_move` fires for ALL handlers - always check `event.bounds.contains(&event.event.position)`
- Gutter content only displays on lines with actual output (check `get_line_timestamp().is_some()`)
