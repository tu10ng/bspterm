# settings

Settings system providing hierarchical configuration with file-based persistence, keymaps, and VS Code import.

## Module Structure

```
src/
├── settings.rs           # Library entry and exports (184 lines)
├── settings_store.rs     # Core settings management (2,499 lines)
├── keymap_file.rs        # Keybinding configuration (2,025 lines)
├── vscode_import.rs      # VS Code settings import (1,064 lines)
├── editorconfig_store.rs # EditorConfig support (393 lines)
├── settings_file.rs      # File watching (159 lines)
├── base_keymap_setting.rs # Base keymap types (135 lines)
├── content_into_gpui.rs  # GPUI type conversion (104 lines)
└── editable_setting_control.rs # UI controls (30 lines)
```

## Key Types

| Type | Purpose |
|------|---------|
| `Settings` | Trait for user setting types |
| `SettingsStore` | Global GPUI entity for settings |
| `EditorconfigStore` | EditorConfig file management |
| `KeymapFile` | Keybinding configuration |
| `SettingsFile` | File source enum (Default/Global/User/Server/Project) |
| `SettingsLocation` | Settings by worktree and path |
| `LocalSettingsKind` | Settings/Tasks/Editorconfig/Debug |

## Settings Hierarchy

Precedence (highest to lowest):
1. **Project Settings** - `.zed/settings.json` in worktrees
2. **Server Settings** - SSH/Telnet specific overrides
3. **User Settings** - `~/.config/bspterm/settings.json`
4. **Global Settings** - Optional global override
5. **Default Settings** - `assets/settings/default.json`

## Dependencies

- `gpui` - Global entity
- `schemars` - JSON schema generation
- `serde_json` - JSON parsing

## Common Tasks

**Define a setting:**
```rust
#[derive(Clone, Deserialize, JsonSchema)]
struct MySetting {
    pub option: bool,
}

impl Settings for MySetting {
    const KEY: Option<&'static str> = Some("my_setting");
    // ...
}
```

**Access a setting:**
```rust
let value = MySetting::get(None, cx);
// or with location
let value = MySetting::get(Some(location), cx);
```

**Register setting type:**
```rust
MySetting::register(cx);
```

## Testing

```sh
cargo test -p settings
```

## Pitfalls

- Settings use `MergeFrom` trait for deep JSON merging
- Profile-based selection via `ActiveSettingsProfileName` global
- Language-specific overrides in `LanguageSettingsContent`
- Release channel and OS-specific overrides supported
- `watch_config_file()` monitors single file changes
- `watch_config_dir()` monitors directory for multiple configs
- Lenient parsing allows trailing commas in JSON
- KeyBinding validation via inventory system
- Context predicates: `"editor && vim_mode == normal"`
- SettingsStore is GPUI Global: `cx.global::<SettingsStore>()`
