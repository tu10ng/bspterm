# gpui

GPU-accelerated UI framework providing state management, concurrency primitives, and rendering. The core abstraction layer for all UI in Bspterm.

## Module Structure

```
src/
├── gpui.rs              # Library root - re-exports all public types
├── app.rs               # Application lifecycle, main App struct
├── app/
│   ├── context.rs       # Context<T> - entity-scoped mutable context
│   ├── async_context.rs # AsyncApp - async-safe context
│   ├── entity_map.rs    # Entity<T>, WeakEntity<T>, EntityId
│   └── test_context.rs  # Testing helpers
├── window.rs            # Window state, event dispatching (5,496 lines)
├── element.rs           # Core Element trait and rendering pipeline
├── view.rs              # AnyView - dynamically-typed view handle
├── elements/            # Pre-built UI components
│   ├── div.rs           # Flex container (142KB, most common)
│   ├── text.rs          # Text rendering
│   ├── list.rs          # Scrollable list with caching
│   ├── uniform_list.rs  # Virtual scrolling
│   ├── img.rs           # Image rendering
│   ├── svg.rs           # SVG rendering
│   ├── canvas.rs        # Custom drawing
│   └── animation.rs     # Frame-based animations
├── style.rs             # CSS-like styling system
├── styled.rs            # Style refinement builder pattern
├── geometry.rs          # Points, Bounds, Size, Rects
├── color.rs             # Color types and manipulation
├── keymap.rs            # Key binding definitions
├── key_dispatch.rs      # Event dispatch tree
├── interactive.rs       # Input events (mouse, keyboard, touch)
├── action.rs            # Action system with macro support
├── executor.rs          # Task scheduling (foreground/background)
├── platform/            # OS-specific (Windows, macOS, Linux)
├── text_system/         # Text layout and font management
└── prelude.rs           # Convenient re-exports
```

## Key Types

| Type | Purpose |
|------|---------|
| `App` | Root context; access to global state, windows, entities |
| `Context<T>` | Entity-scoped context for Entity<T> updates |
| `AsyncApp` | Async-safe context held across await points |
| `Window` | Window state; manages input, layout, painting |
| `Entity<T>` | Strong handle to managed state |
| `WeakEntity<T>` | Weak reference (upgrade to check existence) |
| `Render` | Trait for views: `fn render(&mut self, window, cx) -> impl IntoElement` |
| `RenderOnce` | Trait for components (one-time conversion) |
| `Element` | Trait for layout & paint: `request_layout() → prepaint() → paint()` |
| `Div` | Flexbox container element |
| `UniformList` | Virtual scrolling list |
| `Task<R>` | Future handle (await or detach) |
| `FocusHandle` | Opaque focus management handle |
| `Keystroke` | Parsed keystroke like "ctrl-a" |
| `KeyBinding` | Action mapping with context predicates |

## Dependencies

- `taffy` - Flexbox layout engine
- `cosmic-text` - Text shaping and rendering
- `blade-graphics` / `metal` - GPU rendering backends
- `smol` - Async runtime

## Common Tasks

**Create a view:**
```rust
struct MyView { count: i32 }
impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(format!("Count: {}", self.count))
    }
}
// Create: cx.new(|cx| MyView { count: 0 })
```

**Handle events:**
```rust
div().on_click(cx.listener(|this, event, window, cx| {
    this.count += 1;
    cx.notify();
}))
```

**Spawn async work:**
```rust
cx.spawn(async move |cx| {
    let result = fetch_data().await;
    entity.update(&mut cx, |this, cx| {
        this.data = result;
        cx.notify();
    });
});
```

## Testing

```sh
cargo test -p gpui
```

## Pitfalls

- All entity updates and UI rendering happen on foreground (main) thread
- `cx.notify()` must be called when view state changes to trigger re-render
- Tasks are dropped → cancelled; store in field or detach to prevent
- Trying to update an entity while it's already being updated causes panic
- `cx.spawn` closure receives `WeakEntity<T>` - must upgrade before use
- Event dispatch: Capture phase (root→focused) then Bubble phase (focused→root)
- Use `cx.stop_propagation()` to halt event dispatch
- Prefer GPUI executor timers over `smol::Timer` in tests for `run_until_parked()`
