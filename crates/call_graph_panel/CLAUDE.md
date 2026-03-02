# call_graph_panel

Dockable panel for visualizing call graphs generated from terminal log analysis. Displays DOT-format graphs and provides configuration UI.

## Module Structure

```
src/
├── call_graph_panel.rs   # Main panel (Panel, Focusable, Render traits)
├── code_server_modal.rs  # SSH Docker server configuration modal
├── svg_view.rs           # SVG rendering view (placeholder)
└── trace_config_modal.rs # Configuration modal for rule and server selection
```

## Key Types

| Type | Purpose |
|------|---------|
| `CallGraphPanel` | Main dockable panel implementing `Panel` trait |
| `SvgView` | SVG content renderer with zoom/pan support |
| `TraceConfigModal` | Modal for selecting parse rules and code server |
| `CodeServerModal` | Modal for SSH Docker server configuration |
| `TraceConfig` | Configuration result (rule, code_server_config, log_content) |

## Panel States

The panel displays different views based on `AnalysisProgress`:

| State | Display |
|-------|---------|
| `Idle` | "Select log and trace to generate call graph" |
| `Parsing` | Progress with current/total count |
| `Searching` | Progress with current/total count |
| `Building` | "Building call graph..." |
| `Rendering` | "Rendering..." |
| `Complete` | DOT content in scrollable view |
| `Error` | Error message with retry button |

## Integration

### Initialization

```rust
// In bspterm main.rs
call_graph_panel::init(cx);
```

### Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `ToggleFocus` | `call_graph_panel` | Toggle panel visibility/focus |
| `ExportDot` | `call_graph_panel` | Export graph as DOT file |
| `ExportSvg` | `call_graph_panel` | Export graph as SVG file |
| `TraceCallGraph` | `log_tracer` | Open trace config modal |

### Terminal Context Menu

The "Trace Call Graph" action appears in the terminal right-click menu, triggering the analysis workflow.

## Panel Configuration

| Property | Value |
|----------|-------|
| Position | Right dock (also valid: Left, Bottom) |
| Default width | 400px |
| Icon | `IconName::Folder` |
| Activation priority | 13 |
| Panel key | `"CallGraphPanel"` |

## TraceConfigModal

Configuration modal shown when initiating trace analysis:

- **Log Preview**: Shows line count of selected content
- **Parse Rule**: Button group to select from available rules
- **Code Server**: Displays SSH server, container, and code path with Configure button
- **Actions**: Reload (refresh config), Cancel, Trace buttons

## CodeServerModal

Configuration modal for SSH Docker code server:

- **SSH Host**: Remote server address
- **SSH Port**: Default 22
- **Username**: SSH user (e.g., root)
- **Password**: Optional, with "Save password" checkbox
- **Docker Container**: Container ID/name to run grep in
- **Code Root Path**: Base path for code search (default: `/usr1`)

Configuration is persisted to `~/.config/bspterm/code_server.json`.

## Translations

| Key | English | Chinese |
|-----|---------|---------|
| `call_graph.title` | Call Graph | 调用图 |
| `call_graph.idle` | Select log and trace... | 选择日志并追踪... |
| `call_graph.building` | Building call graph... | 构建调用图中... |
| `call_graph.rendering` | Rendering... | 渲染中... |
| `call_graph.complete` | Call graph complete | 调用图完成 |
| `call_graph.error` | Error | 错误 |
| `call_graph.error_title` | Analysis Failed | 分析失败 |
| `call_graph.export_dot` | Export DOT | 导出 DOT |
| `call_graph.export_svg` | Export SVG | 导出 SVG |

## Dependencies

- `log_tracer` - Analysis engine and data types
- `editor` - Text input fields
- `gpui` - UI framework
- `workspace` - Panel, ModalView, DockPosition
- `ui` - Button, Label, Icon, Checkbox components
- `i18n` - Translations

## Usage Flow

```
1. User right-clicks in terminal
2. Selects "Trace Call Graph"
3. TraceConfigModal opens
4. (First time) User clicks Configure to set up SSH Docker server
5. CodeServerModal opens, user enters server details
6. User saves, modal closes, TraceConfigModal shows server info
7. User selects parse rule and clicks Trace
8. Analysis runs via SSH (panel shows progress)
9. DOT graph displayed in panel
10. User can export DOT/SVG or clear
```

## Testing

```sh
cargo test -p call_graph_panel
```

## Pitfalls

- Panel requires `call_graph_panel::init(cx)` before use
- `TraceConfigModal` loads rules from `LogParseRuleStore` on open
- `CodeServerModal` loads config from `~/.config/bspterm/code_server.json`
- SSH uses password authentication only (no SSH key support currently)
- Trace button is disabled until code server is configured
- DOT content currently displayed as text (SVG rendering is placeholder)
- Panel closes when workspace closes, state is not persisted
- `overflow_y_scroll()` requires `.id()` on parent element first
