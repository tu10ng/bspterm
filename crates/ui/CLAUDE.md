# ui

Shared UI components and patterns. Provides reusable components built on GPUI with consistent styling.

## Module Structure

```
src/
├── ui.rs               # Library entry point
├── prelude.rs          # Common imports re-export
├── components.rs       # Component module exports
├── components/         # 40+ UI components
│   ├── button/         # Button, IconButton, ToggleButton, etc.
│   ├── label/          # Label, HighlightedLabel, LoadingLabel
│   ├── list/           # ListItem, ListHeader, ListSeparator
│   ├── icon/           # Icon, DecoratedIcon
│   ├── avatar.rs       # User avatar display
│   ├── modal.rs        # Modal dialog
│   ├── popover.rs      # Popover container
│   ├── context_menu.rs # Context menu
│   ├── dropdown_menu.rs # Dropdown menu
│   ├── tab.rs          # Tab component
│   ├── tab_bar.rs      # Tab bar container
│   ├── tooltip.rs      # Tooltip display
│   ├── toggle.rs       # Toggle switch
│   ├── checkbox.rs     # Checkbox component
│   ├── disclosure.rs   # Collapsible disclosure
│   └── ...             # Many more components
├── styles/             # Design tokens
│   ├── color.rs        # Color palettes
│   ├── typography.rs   # Font sizes, weights
│   ├── elevation.rs    # Layering/shadows
│   ├── spacing.rs      # Dynamic spacing
│   └── animation.rs    # Animation curves
├── traits/             # Behavioral traits
│   ├── clickable.rs    # on_click handlers
│   ├── disableable.rs  # Disabled state
│   ├── toggleable.rs   # Toggle capability
│   └── styled_ext.rs   # Custom styling
└── utils/              # Utilities
    ├── search_input.rs # Search field
    └── format_distance.rs # Date formatting
```

## Key Types

| Type | Purpose |
|------|---------|
| `Button` | Interactive button with variants |
| `IconButton` | Icon-only button |
| `Label` | Text display with styling |
| `ListItem` | List row with slots |
| `Icon` | SVG icon rendering |
| `Modal` | Modal dialog container |
| `Popover` | Floating popover |
| `ContextMenu` | Right-click menu |
| `Tab` / `TabBar` | Tab navigation |
| `Toggle` | Toggle switch |
| `Disclosure` | Collapsible section |

## Key Traits

| Trait | Purpose |
|-------|---------|
| `Clickable` | Elements with `on_click()` |
| `Disableable` | Elements with disabled state |
| `Toggleable` | Elements with toggle capability |
| `StyledExt` | Custom styling extensions |
| `VisibleOnHover` | Hover-triggered visibility |

## Dependencies

- `gpui` - UI framework foundation
- `theme` - Runtime theme access
- `icons` - IconName enum

## Common Tasks

**Use a button:**
```rust
Button::new("id", "Label")
    .icon(IconName::Check)
    .on_click(|event, window, cx| { /* ... */ })
```

**Create a list item:**
```rust
ListItem::new("id")
    .child(Label::new("Item text"))
    .indent_level(2)
    .on_click(cx.listener(|this, event, window, cx| { /* ... */ }))
```

**Add custom component:**
1. Create struct with `#[derive(IntoElement)]`
2. Implement `RenderOnce` trait
3. Export from `components.rs`

## Testing

```sh
cargo test -p ui
```

## Pitfalls

- All components use fluent builder API (chainable methods)
- Components use `#[derive(IntoElement, RegisterComponent)]`
- `ListItem` has slots: `start_slot`, `end_slot`, `end_hover_slot`
- Event handler signature: `|event, window: &mut Window, cx: &mut App|`
- Colors and sizing respect active theme at runtime
- Use `prelude.rs` for common imports
