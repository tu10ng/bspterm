use std::path::PathBuf;

use alacritty_terminal::vte::ansi::{
    CursorShape as AlacCursorShape, CursorStyle as AlacCursorStyle,
};
use collections::HashMap;
use gpui::{FontFallbacks, FontFeatures, FontWeight, Pixels, px};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use settings::AlternateScroll;
pub use settings::DeviceOnlineAction;
pub use settings::TabDoubleClickAction;

use settings::{
    IntoGpui, PathHyperlinkRegex, RegisterSetting, ShowScrollbar, TerminalBlink,
    TerminalDockPosition, TerminalLineHeight, VenvSettings, WorkingDirectory,
    merge_from::MergeFrom,
};
use task::Shell;
use theme::FontFamilyName;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Toolbar {
    pub breadcrumbs: bool,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BarsSettings {
    pub show_button_bar: bool,
    pub show_function_bar: bool,
    pub show_shortcut_bar: bool,
}

impl Default for BarsSettings {
    fn default() -> Self {
        Self {
            show_button_bar: true,
            show_function_bar: true,
            show_shortcut_bar: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GutterSettings {
    pub line_numbers: bool,
    pub timestamps: bool,
    pub timestamp_format: String,
    pub relative_line_numbers: bool,
}

impl Default for GutterSettings {
    fn default() -> Self {
        Self {
            line_numbers: true,
            timestamps: true,
            timestamp_format: "%H:%M:%S".to_string(),
            relative_line_numbers: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SessionLoggingSettings {
    pub enabled: bool,
    pub log_directory: PathBuf,
    pub filename_pattern: String,
    pub timestamp_format: String,
    pub include_ansi_codes: bool,
}

impl Default for SessionLoggingSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            log_directory: paths::config_dir().join("session_logs"),
            filename_pattern: "${session_name}_%Y-%m-%d_%H.%M.%S_${weekday_cn}.log".to_string(),
            timestamp_format: "[%Y-%m-%d %H:%M:%S] ".to_string(),
            include_ansi_codes: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, RegisterSetting)]
pub struct TerminalSettings {
    pub shell: Shell,
    pub working_directory: WorkingDirectory,
    pub font_size: Option<Pixels>, // todo(settings_refactor) can be non-optional...
    pub font_family: Option<FontFamilyName>,
    pub font_fallbacks: Option<FontFallbacks>,
    pub font_features: Option<FontFeatures>,
    pub font_weight: Option<FontWeight>,
    pub line_height: TerminalLineHeight,
    pub env: HashMap<String, String>,
    pub cursor_shape: CursorShape,
    pub blinking: TerminalBlink,
    pub alternate_scroll: AlternateScroll,
    pub option_as_meta: bool,
    pub copy_on_select: bool,
    pub keep_selection_on_copy: bool,
    pub button: bool,
    pub dock: TerminalDockPosition,
    pub default_width: Pixels,
    pub default_height: Pixels,
    pub detect_venv: VenvSettings,
    pub max_scroll_history_lines: Option<usize>,
    pub scroll_multiplier: f32,
    pub toolbar: Toolbar,
    pub scrollbar: ScrollbarSettings,
    pub gutter: GutterSettings,
    pub session_logging: SessionLoggingSettings,
    pub minimum_contrast: f32,
    pub path_hyperlink_regexes: Vec<String>,
    pub path_hyperlink_timeout_ms: u64,
    pub send_keybindings_to_shell: bool,
    pub keybindings_to_skip_shell: Vec<String>,
    pub connection_timeout_secs: u64,
    pub auto_reconnect: bool,
    pub notify_on_reconnect: bool,
    pub recently_active_timeout_secs: u64,
    pub ping_timeout_secs: u64,
    pub tab_double_click_action: TabDoubleClickAction,
    pub device_online_action: DeviceOnlineAction,
    pub device_online_script: Option<String>,
    pub ssh_keepalive_interval_secs: u64,
    pub ssh_keepalive_max: usize,
    pub group_tabs_by_session: bool,
    pub autosuggestion: bool,
    pub autosuggestion_max_age_days: u64,
    pub bars: BarsSettings,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ScrollbarSettings {
    /// When to show the scrollbar in the terminal.
    ///
    /// Default: inherits editor scrollbar settings
    pub show: Option<ShowScrollbar>,
}

fn settings_shell_to_task_shell(shell: settings::Shell) -> Shell {
    match shell {
        settings::Shell::System => Shell::System,
        settings::Shell::Program(program) => Shell::Program(program),
        settings::Shell::WithArguments {
            program,
            args,
            title_override,
        } => Shell::WithArguments {
            program,
            args,
            title_override,
        },
    }
}

impl settings::Settings for TerminalSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let user_content = content.terminal.clone().unwrap();
        // Note: we allow a subset of "terminal" settings in the project files.
        let mut project_content = user_content.project.clone();
        project_content.merge_from_option(content.project.terminal.as_ref());
        TerminalSettings {
            shell: settings_shell_to_task_shell(project_content.shell.unwrap()),
            working_directory: project_content.working_directory.unwrap(),
            font_size: user_content.font_size.map(|s| s.into_gpui()),
            font_family: user_content.font_family,
            font_fallbacks: user_content.font_fallbacks.map(|fallbacks| {
                FontFallbacks::from_fonts(
                    fallbacks
                        .into_iter()
                        .map(|family| family.0.to_string())
                        .collect(),
                )
            }),
            font_features: user_content.font_features.map(|f| f.into_gpui()),
            font_weight: user_content.font_weight.map(|w| w.into_gpui()),
            line_height: user_content.line_height.unwrap(),
            env: project_content.env.unwrap(),
            cursor_shape: user_content.cursor_shape.unwrap().into(),
            blinking: user_content.blinking.unwrap(),
            alternate_scroll: user_content.alternate_scroll.unwrap(),
            option_as_meta: user_content.option_as_meta.unwrap(),
            copy_on_select: user_content.copy_on_select.unwrap(),
            keep_selection_on_copy: user_content.keep_selection_on_copy.unwrap(),
            button: user_content.button.unwrap(),
            dock: user_content.dock.unwrap(),
            default_width: px(user_content.default_width.unwrap()),
            default_height: px(user_content.default_height.unwrap()),
            detect_venv: project_content.detect_venv.unwrap(),
            scroll_multiplier: user_content.scroll_multiplier.unwrap(),
            max_scroll_history_lines: user_content.max_scroll_history_lines,
            toolbar: Toolbar {
                breadcrumbs: user_content.toolbar.unwrap().breadcrumbs.unwrap(),
            },
            scrollbar: ScrollbarSettings {
                show: user_content.scrollbar.unwrap().show,
            },
            gutter: {
                let gutter_content = user_content.gutter.unwrap_or_default();
                GutterSettings {
                    line_numbers: gutter_content.line_numbers.unwrap_or(true),
                    timestamps: gutter_content.timestamps.unwrap_or(true),
                    timestamp_format: gutter_content
                        .timestamp_format
                        .unwrap_or_else(|| "%H:%M:%S".to_string()),
                    relative_line_numbers: gutter_content.relative_line_numbers.unwrap_or(false),
                }
            },
            session_logging: {
                let logging_content = user_content.session_logging.unwrap_or_default();
                let default = SessionLoggingSettings::default();
                SessionLoggingSettings {
                    enabled: logging_content.enabled.unwrap_or(default.enabled),
                    log_directory: logging_content
                        .log_directory
                        .map(|s| PathBuf::from(s))
                        .unwrap_or(default.log_directory),
                    filename_pattern: logging_content
                        .filename_pattern
                        .unwrap_or(default.filename_pattern),
                    timestamp_format: logging_content
                        .timestamp_format
                        .unwrap_or(default.timestamp_format),
                    include_ansi_codes: logging_content
                        .include_ansi_codes
                        .unwrap_or(default.include_ansi_codes),
                }
            },
            minimum_contrast: user_content.minimum_contrast.unwrap(),
            path_hyperlink_regexes: project_content
                .path_hyperlink_regexes
                .unwrap()
                .into_iter()
                .map(|regex| match regex {
                    PathHyperlinkRegex::SingleLine(regex) => regex,
                    PathHyperlinkRegex::MultiLine(regex) => regex.join("\n"),
                })
                .collect(),
            path_hyperlink_timeout_ms: project_content.path_hyperlink_timeout_ms.unwrap(),
            send_keybindings_to_shell: user_content.send_keybindings_to_shell.unwrap(),
            keybindings_to_skip_shell: user_content.keybindings_to_skip_shell.unwrap(),
            connection_timeout_secs: user_content.connection_timeout_secs.unwrap_or(3),
            auto_reconnect: user_content.auto_reconnect.unwrap_or(true),
            notify_on_reconnect: user_content.notify_on_reconnect.unwrap_or(true),
            recently_active_timeout_secs: user_content.recently_active_timeout_secs.unwrap_or(60),
            ping_timeout_secs: user_content.ping_timeout_secs.unwrap_or(10),
            tab_double_click_action: user_content
                .tab_double_click_action
                .unwrap_or(TabDoubleClickAction::Duplicate),
            device_online_action: user_content
                .device_online_action
                .unwrap_or(DeviceOnlineAction::Notify),
            device_online_script: user_content.device_online_script,
            ssh_keepalive_interval_secs: user_content.ssh_keepalive_interval_secs.unwrap_or(5),
            ssh_keepalive_max: user_content.ssh_keepalive_max.unwrap_or(2),
            group_tabs_by_session: user_content.group_tabs_by_session.unwrap_or(true),
            autosuggestion: user_content.autosuggestion.unwrap_or(true),
            autosuggestion_max_age_days: user_content.autosuggestion_max_age_days.unwrap_or(7),
            bars: {
                let default = BarsSettings::default();
                match user_content.bars {
                    Some(bars_content) => BarsSettings {
                        show_button_bar: bars_content
                            .show_button_bar
                            .unwrap_or(default.show_button_bar),
                        show_function_bar: bars_content
                            .show_function_bar
                            .unwrap_or(default.show_function_bar),
                        show_shortcut_bar: bars_content
                            .show_shortcut_bar
                            .unwrap_or(default.show_shortcut_bar),
                    },
                    None => default,
                }
            },
        }
    }
}

impl TerminalSettings {
    pub fn connection_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.connection_timeout_secs)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    /// Cursor is a block like `█`.
    #[default]
    Block,
    /// Cursor is an underscore like `_`.
    Underline,
    /// Cursor is a vertical bar like `⎸`.
    Bar,
    /// Cursor is a hollow box like `▯`.
    Hollow,
}

impl From<settings::CursorShapeContent> for CursorShape {
    fn from(value: settings::CursorShapeContent) -> Self {
        match value {
            settings::CursorShapeContent::Block => CursorShape::Block,
            settings::CursorShapeContent::Underline => CursorShape::Underline,
            settings::CursorShapeContent::Bar => CursorShape::Bar,
            settings::CursorShapeContent::Hollow => CursorShape::Hollow,
        }
    }
}

impl From<CursorShape> for AlacCursorShape {
    fn from(value: CursorShape) -> Self {
        match value {
            CursorShape::Block => AlacCursorShape::Block,
            CursorShape::Underline => AlacCursorShape::Underline,
            CursorShape::Bar => AlacCursorShape::Beam,
            CursorShape::Hollow => AlacCursorShape::HollowBlock,
        }
    }
}

impl From<CursorShape> for AlacCursorStyle {
    fn from(value: CursorShape) -> Self {
        AlacCursorStyle {
            shape: value.into(),
            blinking: false,
        }
    }
}
