use std::sync::Arc;

use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla, WindowBackgroundAppearance, hsla};

use crate::{
    AccentColors, Appearance, DEFAULT_DARK_THEME, PlayerColors, StatusColors,
    StatusColorsRefinement, SyntaxTheme, SystemColors, Theme, ThemeColors, ThemeColorsRefinement,
    ThemeFamily, ThemeStyles, default_color_scales,
};

/// The default theme family for Zed.
///
/// This is used to construct the default theme fallback values, as well as to
/// have a theme available at compile time for tests.
pub fn zed_default_themes() -> ThemeFamily {
    ThemeFamily {
        id: "zed-default".to_string(),
        name: "Zed Default".into(),
        author: "".into(),
        themes: vec![zed_default_dark()],
        scales: default_color_scales(),
    }
}

// If a theme customizes a foreground version of a status color, but does not
// customize the background color, then use a partly-transparent version of the
// foreground color for the background color.
pub(crate) fn apply_status_color_defaults(status: &mut StatusColorsRefinement) {
    for (fg_color, bg_color) in [
        (&status.deleted, &mut status.deleted_background),
        (&status.created, &mut status.created_background),
        (&status.modified, &mut status.modified_background),
        (&status.conflict, &mut status.conflict_background),
        (&status.error, &mut status.error_background),
        (&status.hidden, &mut status.hidden_background),
    ] {
        if bg_color.is_none()
            && let Some(fg_color) = fg_color
        {
            *bg_color = Some(fg_color.opacity(0.25));
        }
    }
}

pub(crate) fn apply_theme_color_defaults(
    theme_colors: &mut ThemeColorsRefinement,
    player_colors: &PlayerColors,
) {
    if theme_colors.element_selection_background.is_none() {
        let mut selection = player_colors.local().selection;
        if selection.a == 1.0 {
            selection.a = 0.25;
        }
        theme_colors.element_selection_background = Some(selection);
    }
}

pub(crate) fn zed_default_dark() -> Theme {
    // VSCode Dark+ style colors matching Bspterm Dark theme
    let bg = hsla(0. / 360., 0. / 100., 18. / 100., 1.); // #2d2d2d
    let editor = hsla(0. / 360., 0. / 100., 12. / 100., 1.); // #1e1e1e
    let elevated_surface = hsla(0. / 360., 0. / 100., 15. / 100., 1.); // #252526
    let hover = hsla(200. / 360., 4. / 100., 20. / 100., 1.0); // #2a2d2e

    let blue = hsla(210. / 360., 60. / 100., 59. / 100., 1.0); // #569cd6
    let gray = hsla(0. / 360., 0. / 100., 43. / 100., 1.0); // #6d6d6d
    let green = hsla(100. / 360., 26. / 100., 47. / 100., 1.0); // #6a9955
    let orange = hsla(19. / 360., 52. / 100., 64. / 100., 1.0); // #ce9178
    let purple = hsla(300. / 360., 30. / 100., 68. / 100., 1.0); // #c586c0
    let red = hsla(0. / 360., 86. / 100., 62. / 100., 1.0); // #f44747
    let teal = hsla(160. / 360., 56. / 100., 55. / 100., 1.0); // #4ec9b0
    let yellow = hsla(51. / 360., 53. / 100., 76. / 100., 1.0); // #dcdcaa

    const ADDED_COLOR: Hsla = Hsla {
        h: 134. / 360.,
        s: 0.55,
        l: 0.40,
        a: 1.0,
    };
    const WORD_ADDED_COLOR: Hsla = Hsla {
        h: 134. / 360.,
        s: 0.55,
        l: 0.40,
        a: 0.35,
    };
    const MODIFIED_COLOR: Hsla = Hsla {
        h: 48. / 360.,
        s: 0.76,
        l: 0.47,
        a: 1.0,
    };
    const REMOVED_COLOR: Hsla = Hsla {
        h: 350. / 360.,
        s: 0.88,
        l: 0.25,
        a: 1.0,
    };
    const WORD_DELETED_COLOR: Hsla = Hsla {
        h: 350. / 360.,
        s: 0.88,
        l: 0.25,
        a: 0.80,
    };

    let player = PlayerColors::dark();
    Theme {
        id: "bspterm_dark".to_string(),
        name: DEFAULT_DARK_THEME.into(),
        appearance: Appearance::Dark,
        styles: ThemeStyles {
            window_background_appearance: WindowBackgroundAppearance::Opaque,
            system: SystemColors::default(),
            accents: AccentColors(vec![blue, orange, purple, teal, red, green, yellow]),
            colors: ThemeColors {
                border: hsla(0. / 360., 0. / 100., 25. / 100., 1.), // #3f3f3f
                border_variant: hsla(0. / 360., 0. / 100., 18. / 100., 1.), // #2d2d2d
                border_focused: hsla(200. / 360., 100. / 100., 40. / 100., 1.), // #007acc
                border_selected: hsla(207. / 360., 84. / 100., 18. / 100., 1.0), // #094771
                border_transparent: SystemColors::default().transparent,
                border_disabled: hsla(0. / 360., 0. / 100., 25. / 100., 1.0), // #3f3f3f
                elevated_surface_background: elevated_surface,
                surface_background: bg,
                background: bg,
                element_background: hsla(0. / 360., 0. / 100., 18. / 100., 1.0), // #2d2d2d
                element_hover: hover,
                element_active: hsla(240. / 360., 3. / 100., 22. / 100., 1.0), // #37373d
                element_selected: hsla(240. / 360., 3. / 100., 22. / 100., 1.0), // #37373d
                element_disabled: SystemColors::default().transparent,
                element_selection_background: player.local().selection.alpha(0.25),
                drop_target_background: hsla(220.0 / 360., 8.3 / 100., 21.4 / 100., 1.0),
                drop_target_border: hsla(221. / 360., 11. / 100., 86. / 100., 1.0),
                ghost_element_background: SystemColors::default().transparent,
                ghost_element_hover: hover,
                ghost_element_active: hsla(240. / 360., 3. / 100., 22. / 100., 1.0),
                ghost_element_selected: hsla(240. / 360., 3. / 100., 22. / 100., 1.0),
                ghost_element_disabled: SystemColors::default().transparent,
                text: hsla(0. / 360., 0. / 100., 83. / 100., 1.0), // #d4d4d4
                text_muted: hsla(0. / 360., 0. / 100., 62. / 100., 1.0), // #9d9d9d
                text_placeholder: hsla(0. / 360., 0. / 100., 43. / 100., 1.0), // #6d6d6d
                text_disabled: hsla(0. / 360., 0. / 100., 43. / 100., 1.0), // #6d6d6d
                text_accent: hsla(210. / 360., 60. / 100., 59. / 100., 1.0), // #569cd6
                icon: hsla(0. / 360., 0. / 100., 83. / 100., 1.0), // #d4d4d4
                icon_muted: hsla(0. / 360., 0. / 100., 62. / 100., 1.0), // #9d9d9d
                icon_disabled: hsla(0. / 360., 0. / 100., 43. / 100., 1.0), // #6d6d6d
                icon_placeholder: hsla(0. / 360., 0. / 100., 62. / 100., 1.0), // #9d9d9d
                icon_accent: blue,
                debugger_accent: red,
                status_bar_background: bg,
                title_bar_background: bg,
                title_bar_inactive_background: bg,
                toolbar_background: editor,
                tab_bar_background: bg,
                tab_inactive_background: bg,
                tab_active_background: editor,
                search_match_background: bg,
                search_active_match_background: bg,

                editor_background: editor,
                editor_gutter_background: editor,
                editor_subheader_background: bg,
                editor_active_line_background: hsla(222.9 / 360., 13.5 / 100., 20.4 / 100., 1.0),
                editor_highlighted_line_background: hsla(207.8 / 360., 81. / 100., 66. / 100., 0.1),
                editor_debugger_active_line_background: hsla(
                    207.8 / 360.,
                    81. / 100.,
                    66. / 100.,
                    0.2,
                ),
                editor_line_number: hsla(0. / 360., 0. / 100., 52. / 100., 1.0), // #858585
                editor_active_line_number: hsla(0. / 360., 0. / 100., 78. / 100., 1.0), // #c6c6c6
                editor_hover_line_number: hsla(0. / 360., 0. / 100., 63. / 100., 1.0), // #a0a0a0
                editor_invisible: hsla(0. / 360., 0. / 100., 43. / 100., 1.0), // #6d6d6d
                editor_wrap_guide: hsla(0. / 360., 0. / 100., 83. / 100., 0.05),
                editor_active_wrap_guide: hsla(0. / 360., 0. / 100., 83. / 100., 0.1),
                editor_indent_guide: hsla(0. / 360., 0. / 100., 25. / 100., 1.),
                editor_indent_guide_active: hsla(0. / 360., 0. / 100., 35. / 100., 1.),
                editor_document_highlight_read_background: hsla(
                    207.8 / 360.,
                    81. / 100.,
                    66. / 100.,
                    0.2,
                ),
                editor_document_highlight_write_background: gpui::red(),
                editor_document_highlight_bracket_background: gpui::green(),

                terminal_background: bg,
                // todo("Use one colors for terminal")
                terminal_ansi_background: crate::black().dark().step_12(),
                terminal_foreground: crate::white().dark().step_12(),
                terminal_bright_foreground: crate::white().dark().step_11(),
                terminal_dim_foreground: crate::white().dark().step_10(),
                terminal_ansi_black: crate::black().dark().step_12(),
                terminal_ansi_red: crate::red().dark().step_11(),
                terminal_ansi_green: crate::green().dark().step_11(),
                terminal_ansi_yellow: crate::yellow().dark().step_11(),
                terminal_ansi_blue: crate::blue().dark().step_11(),
                terminal_ansi_magenta: crate::violet().dark().step_11(),
                terminal_ansi_cyan: crate::cyan().dark().step_11(),
                terminal_ansi_white: crate::neutral().dark().step_12(),
                terminal_ansi_bright_black: crate::black().dark().step_11(),
                terminal_ansi_bright_red: crate::red().dark().step_10(),
                terminal_ansi_bright_green: crate::green().dark().step_10(),
                terminal_ansi_bright_yellow: crate::yellow().dark().step_10(),
                terminal_ansi_bright_blue: crate::blue().dark().step_10(),
                terminal_ansi_bright_magenta: crate::violet().dark().step_10(),
                terminal_ansi_bright_cyan: crate::cyan().dark().step_10(),
                terminal_ansi_bright_white: crate::neutral().dark().step_11(),
                terminal_ansi_dim_black: crate::black().dark().step_10(),
                terminal_ansi_dim_red: crate::red().dark().step_9(),
                terminal_ansi_dim_green: crate::green().dark().step_9(),
                terminal_ansi_dim_yellow: crate::yellow().dark().step_9(),
                terminal_ansi_dim_blue: crate::blue().dark().step_9(),
                terminal_ansi_dim_magenta: crate::violet().dark().step_9(),
                terminal_ansi_dim_cyan: crate::cyan().dark().step_9(),
                terminal_ansi_dim_white: crate::neutral().dark().step_10(),
                terminal_tab_active_background: crate::indigo().dark().step_4(),
                terminal_gutter_separator: crate::neutral().dark().step_6(),
                panel_background: bg,
                panel_focused_border: blue,
                panel_indent_guide: hsla(0. / 360., 0. / 100., 25. / 100., 1.),
                panel_indent_guide_hover: hsla(0. / 360., 0. / 100., 35. / 100., 1.),
                panel_indent_guide_active: hsla(0. / 360., 0. / 100., 35. / 100., 1.),
                panel_overlay_background: bg,
                panel_overlay_hover: hover,
                pane_focused_border: blue,
                pane_group_border: hsla(0. / 360., 0. / 100., 25. / 100., 1.),
                scrollbar_thumb_background: hsla(0. / 360., 0. / 100., 47. / 100., 0.4), // #797979 40%
                scrollbar_thumb_hover_background: hsla(0. / 360., 0. / 100., 47. / 100., 0.7),
                scrollbar_thumb_active_background: hsla(0. / 360., 0. / 100., 47. / 100., 1.0),
                scrollbar_thumb_border: hsla(0. / 360., 0. / 100., 47. / 100., 0.4),
                scrollbar_track_background: gpui::transparent_black(),
                scrollbar_track_border: hsla(0. / 360., 0. / 100., 15. / 100., 1.),
                minimap_thumb_background: hsla(0. / 360., 0. / 100., 30. / 100., 0.7),
                minimap_thumb_hover_background: hsla(0. / 360., 0. / 100., 30. / 100., 0.8),
                minimap_thumb_active_background: hsla(0. / 360., 0. / 100., 30. / 100., 0.9),
                minimap_thumb_border: hsla(0. / 360., 0. / 100., 25. / 100., 1.),
                editor_foreground: hsla(0. / 360., 0. / 100., 83. / 100., 1.), // #d4d4d4
                link_text_hover: blue,
                version_control_added: ADDED_COLOR,
                version_control_deleted: REMOVED_COLOR,
                version_control_modified: MODIFIED_COLOR,
                version_control_renamed: MODIFIED_COLOR,
                version_control_conflict: crate::orange().light().step_12(),
                version_control_ignored: crate::gray().light().step_12(),
                version_control_word_added: WORD_ADDED_COLOR,
                version_control_word_deleted: WORD_DELETED_COLOR,
                version_control_conflict_marker_ours: crate::green().light().step_12().alpha(0.5),
                version_control_conflict_marker_theirs: crate::blue().light().step_12().alpha(0.5),

                vim_normal_background: SystemColors::default().transparent,
                vim_insert_background: SystemColors::default().transparent,
                vim_replace_background: SystemColors::default().transparent,
                vim_visual_background: SystemColors::default().transparent,
                vim_visual_line_background: SystemColors::default().transparent,
                vim_visual_block_background: SystemColors::default().transparent,
                vim_helix_normal_background: SystemColors::default().transparent,
                vim_helix_select_background: SystemColors::default().transparent,
                vim_normal_foreground: SystemColors::default().transparent,
                vim_insert_foreground: SystemColors::default().transparent,
                vim_replace_foreground: SystemColors::default().transparent,
                vim_visual_foreground: SystemColors::default().transparent,
                vim_visual_line_foreground: SystemColors::default().transparent,
                vim_visual_block_foreground: SystemColors::default().transparent,
                vim_helix_normal_foreground: SystemColors::default().transparent,
                vim_helix_select_foreground: SystemColors::default().transparent,
            },
            status: StatusColors {
                conflict: yellow,
                conflict_background: yellow,
                conflict_border: yellow,
                created: green,
                created_background: green,
                created_border: green,
                deleted: red,
                deleted_background: red,
                deleted_border: red,
                error: red,
                error_background: red,
                error_border: red,
                hidden: gray,
                hidden_background: gray,
                hidden_border: gray,
                hint: blue,
                hint_background: blue,
                hint_border: blue,
                ignored: gray,
                ignored_background: gray,
                ignored_border: gray,
                info: blue,
                info_background: blue,
                info_border: blue,
                modified: yellow,
                modified_background: yellow,
                modified_border: yellow,
                predictive: gray,
                predictive_background: gray,
                predictive_border: gray,
                renamed: blue,
                renamed_background: blue,
                renamed_border: blue,
                success: green,
                success_background: green,
                success_border: green,
                unreachable: gray,
                unreachable_background: gray,
                unreachable_border: gray,
                warning: yellow,
                warning_background: yellow,
                warning_border: yellow,
            },
            player,
            syntax: Arc::new(SyntaxTheme {
                highlights: vec![
                    ("attribute".into(), purple.into()),
                    ("boolean".into(), orange.into()),
                    ("comment".into(), gray.into()),
                    ("comment.doc".into(), gray.into()),
                    ("constant".into(), yellow.into()),
                    ("constructor".into(), blue.into()),
                    ("embedded".into(), HighlightStyle::default()),
                    (
                        "emphasis".into(),
                        HighlightStyle {
                            font_style: Some(FontStyle::Italic),
                            ..HighlightStyle::default()
                        },
                    ),
                    (
                        "emphasis.strong".into(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..HighlightStyle::default()
                        },
                    ),
                    ("enum".into(), teal.into()),
                    ("function".into(), blue.into()),
                    ("function.method".into(), blue.into()),
                    ("function.definition".into(), blue.into()),
                    ("hint".into(), blue.into()),
                    ("keyword".into(), purple.into()),
                    ("label".into(), HighlightStyle::default()),
                    ("link_text".into(), blue.into()),
                    (
                        "link_uri".into(),
                        HighlightStyle {
                            color: Some(teal),
                            font_style: Some(FontStyle::Italic),
                            ..HighlightStyle::default()
                        },
                    ),
                    ("number".into(), orange.into()),
                    ("operator".into(), HighlightStyle::default()),
                    ("predictive".into(), HighlightStyle::default()),
                    ("preproc".into(), HighlightStyle::default()),
                    ("primary".into(), HighlightStyle::default()),
                    ("property".into(), red.into()),
                    ("punctuation".into(), HighlightStyle::default()),
                    ("punctuation.bracket".into(), HighlightStyle::default()),
                    ("punctuation.delimiter".into(), HighlightStyle::default()),
                    ("punctuation.list_marker".into(), HighlightStyle::default()),
                    ("punctuation.special".into(), HighlightStyle::default()),
                    ("string".into(), green.into()),
                    ("string.escape".into(), HighlightStyle::default()),
                    ("string.regex".into(), red.into()),
                    ("string.special".into(), HighlightStyle::default()),
                    ("string.special.symbol".into(), HighlightStyle::default()),
                    ("tag".into(), HighlightStyle::default()),
                    ("text.literal".into(), HighlightStyle::default()),
                    ("title".into(), HighlightStyle::default()),
                    ("type".into(), teal.into()),
                    ("variable".into(), HighlightStyle::default()),
                    ("variable.special".into(), red.into()),
                    ("variant".into(), HighlightStyle::default()),
                ],
            }),
        },
    }
}
