# workspace

Window management, local state serialization, and project grouping. Manages the overall application layout with docks, panes, and panels.

## Module Structure

```
src/
├── workspace.rs        # Main Workspace entity (12,480 lines)
├── dock.rs             # Dock container management
├── pane.rs             # Pane container for items (8,194 lines)
├── pane_group.rs       # Hierarchical pane splitting
├── item.rs             # Item trait for tab content
├── utility_pane.rs     # Side utility panes
├── persistence.rs      # Window/workspace serialization (3,558 lines)
├── notifications.rs    # Toast/notification system
├── searchable.rs       # Search interface for items
├── toolbar.rs          # Toolbar and status bar items
├── history_manager.rs  # Navigation history
├── modal_layer.rs      # Modal dialog layer
├── toast_layer.rs      # Toast notification layer
├── activity_bar.rs     # Activity bar UI
├── status_bar.rs       # Status bar UI
└── shared_screen.rs    # Collaboration screen sharing
```

## Key Types

| Type | Purpose |
|------|---------|
| `Workspace` | Top-level entity managing window layout |
| `Pane` | Container for items with tabs and history |
| `PaneGroup` | Hierarchical split layout (Member tree) |
| `Dock` | Container at DockPosition (Left/Bottom/Right) |
| `DockPosition` | Left, Bottom, Right enum |
| `Panel` | Trait for dock panels |
| `PanelHandle` | Type-erased panel handle |
| `Item` | Trait for pane tab content |
| `ItemHandle` | Type-erased item handle |
| `Member` | Pane or Axis in split tree |
| `SplitDirection` | Horizontal, Vertical |

## Dependencies

- `gpui` - UI framework
- `project` - File/LSP management
- `db` - State persistence
- `settings` - Configuration

## Common Tasks

**Register a panel:**
```rust
// In crate init:
cx.observe_new_views(|workspace: &mut Workspace, window, cx| {
    workspace.register_action(window, |ws, _: &ToggleFocus, window, cx| {
        ws.toggle_panel_focus::<MyPanel>(window, cx);
    });
});

// Panel must implement Panel trait
impl Panel for MyPanel {
    fn persistent_name() -> &'static str { "MyPanel" }
    fn position(&self, _cx: &App) -> DockPosition { DockPosition::Left }
    // ...
}
```

**Add item to pane:**
```rust
workspace.add_item_to_active_pane(Box::new(my_item), None, true, window, cx);
```

**Split pane:**
```rust
pane.split(SplitDirection::Right, window, cx);
```

## Testing

```sh
cargo test -p workspace
```

## Pitfalls

- Workspace state saved to `~/.config/bspterm/` for restoration
- Panel trait requires: `persistent_name()`, `panel_key()`, `position()`, `icon()`, `toggle_action()`
- `zoomed` state allows a panel/pane to take full workspace
- Three persistent docks: left_dock, right_dock, bottom_dock
- Item trait requires: `tab_content()`, `navigate()`, `search()`
- SerializableItem subset enables persistence
- `center: PaneGroup` is the central editor area (splittable)
