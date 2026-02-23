use gpui::{App, Menu, MenuItem, OsAction};
use i18n::t;
use release_channel::ReleaseChannel;
use terminal_view::terminal_panel;
use bspterm_actions::dev;

pub fn app_menus(cx: &mut App) -> Vec<Menu> {
    use bspterm_actions::Quit;

    let mut view_items = vec![
        MenuItem::action(
            t("menu.zoom_in"),
            bspterm_actions::IncreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("menu.zoom_out"),
            bspterm_actions::DecreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("menu.reset_zoom"),
            bspterm_actions::ResetBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("menu.reset_all_zoom"),
            bspterm_actions::ResetAllZoom { persist: false },
        ),
        MenuItem::separator(),
        MenuItem::action(t("menu.toggle_left_dock"), workspace::ToggleLeftDock),
        MenuItem::action(t("menu.toggle_right_dock"), workspace::ToggleRightDock),
        MenuItem::action(t("menu.toggle_bottom_dock"), workspace::ToggleBottomDock),
        MenuItem::action(t("menu.toggle_all_docks"), workspace::ToggleAllDocks),
        MenuItem::submenu(Menu {
            name: t("menu.editor_layout"),
            items: vec![
                MenuItem::action(t("menu.split_up"), workspace::SplitUp::default()),
                MenuItem::action(t("menu.split_down"), workspace::SplitDown::default()),
                MenuItem::action(t("menu.split_left"), workspace::SplitLeft::default()),
                MenuItem::action(t("menu.split_right"), workspace::SplitRight::default()),
            ],
        }),
        MenuItem::separator(),
        MenuItem::action(t("menu.remote_explorer"), bspterm_actions::remote_explorer::ToggleFocus),
        MenuItem::action(t("menu.terminal_panel"), terminal_panel::ToggleFocus),
        MenuItem::action(t("menu.outline_panel"), outline_panel::ToggleFocus),
        MenuItem::action(t("menu.project_panel"), bspterm_actions::project_panel::ToggleFocus),
        MenuItem::separator(),
    ];

    if ReleaseChannel::try_global(cx) == Some(ReleaseChannel::Dev) {
        view_items.push(MenuItem::action(
            t("menu.toggle_gpui_inspector"),
            dev::ToggleInspector,
        ));
        view_items.push(MenuItem::separator());
    }

    vec![
        Menu {
            name: t("menu.bspterm"),
            items: vec![
                MenuItem::action(t("menu.about_bspterm"), bspterm_actions::About),
                MenuItem::action(t("menu.check_for_updates"), auto_update::Check),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: t("menu.settings"),
                    items: vec![
                        MenuItem::action(t("menu.open_settings"), bspterm_actions::OpenSettings),
                        MenuItem::action(t("menu.open_settings_file"), super::OpenSettingsFile),
                        MenuItem::action(t("menu.open_project_settings"), bspterm_actions::OpenProjectSettings),
                        MenuItem::action(
                            t("menu.open_project_settings_file"),
                            super::OpenProjectSettingsFile,
                        ),
                        MenuItem::action(t("menu.open_default_settings"), super::OpenDefaultSettings),
                        MenuItem::separator(),
                        MenuItem::action(t("menu.open_keymap"), bspterm_actions::OpenKeymap),
                        MenuItem::action(t("menu.open_keymap_file"), bspterm_actions::OpenKeymapFile),
                        MenuItem::action(
                            t("menu.open_default_key_bindings"),
                            bspterm_actions::OpenDefaultKeymap,
                        ),
                        MenuItem::separator(),
                        MenuItem::action(
                            t("menu.select_theme"),
                            bspterm_actions::theme_selector::Toggle::default(),
                        ),
                        MenuItem::action(
                            t("menu.select_icon_theme"),
                            bspterm_actions::icon_theme_selector::Toggle::default(),
                        ),
                    ],
                }),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(t("menu.extensions"), bspterm_actions::Extensions::default()),
                #[cfg(not(target_os = "windows"))]
                MenuItem::action(t("menu.install_cli"), install_cli::InstallCliBinary),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("menu.hide_bspterm"), super::Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("menu.hide_others"), super::HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("menu.show_all"), super::ShowAll),
                MenuItem::separator(),
                MenuItem::action(t("menu.quit_bspterm"), Quit),
            ],
        },
        Menu {
            name: t("menu.file"),
            items: vec![
                MenuItem::action(t("menu.new"), workspace::NewFile),
                MenuItem::action(t("menu.new_window"), workspace::NewWindow),
                MenuItem::separator(),
                MenuItem::action(
                    t("menu.open_remote"),
                    bspterm_actions::OpenRemote {
                        create_new_window: false,
                        from_existing_connection: false,
                    },
                ),
                #[cfg(not(target_os = "macos"))]
                MenuItem::action(t("menu.open_file"), workspace::OpenFiles),
                MenuItem::separator(),
                MenuItem::action(t("menu.save"), workspace::Save { save_intent: None }),
                MenuItem::action(t("menu.save_as"), workspace::SaveAs),
                MenuItem::action(t("menu.save_all"), workspace::SaveAll { save_intent: None }),
                MenuItem::separator(),
                MenuItem::action(
                    t("menu.close_editor"),
                    workspace::CloseActiveItem {
                        save_intent: None,
                        close_pinned: true,
                    },
                ),
                MenuItem::action(t("menu.close_window"), workspace::CloseWindow),
            ],
        },
        Menu {
            name: t("menu.edit"),
            items: vec![
                MenuItem::os_action(t("menu.undo"), editor::actions::Undo, OsAction::Undo),
                MenuItem::os_action(t("menu.redo"), editor::actions::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action(t("menu.cut"), editor::actions::Cut, OsAction::Cut),
                MenuItem::os_action(t("menu.copy"), editor::actions::Copy, OsAction::Copy),
                MenuItem::action(t("menu.copy_and_trim"), editor::actions::CopyAndTrim),
                MenuItem::os_action(t("menu.paste"), editor::actions::Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::action(t("menu.find"), search::buffer_search::Deploy::find()),
                MenuItem::separator(),
                MenuItem::action(
                    t("menu.toggle_line_comment"),
                    editor::actions::ToggleComments::default(),
                ),
            ],
        },
        Menu {
            name: t("menu.selection"),
            items: vec![
                MenuItem::os_action(
                    t("menu.select_all"),
                    editor::actions::SelectAll,
                    OsAction::SelectAll,
                ),
                MenuItem::action(t("menu.expand_selection"), editor::actions::SelectLargerSyntaxNode),
                MenuItem::action(t("menu.shrink_selection"), editor::actions::SelectSmallerSyntaxNode),
                MenuItem::action(t("menu.select_next_sibling"), editor::actions::SelectNextSyntaxNode),
                MenuItem::action(
                    t("menu.select_previous_sibling"),
                    editor::actions::SelectPreviousSyntaxNode,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    t("menu.add_cursor_above"),
                    editor::actions::AddSelectionAbove {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    t("menu.add_cursor_below"),
                    editor::actions::AddSelectionBelow {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    t("menu.select_next_occurrence"),
                    editor::actions::SelectNext {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(
                    t("menu.select_previous_occurrence"),
                    editor::actions::SelectPrevious {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(t("menu.select_all_occurrences"), editor::actions::SelectAllMatches),
                MenuItem::separator(),
                MenuItem::action(t("menu.move_line_up"), editor::actions::MoveLineUp),
                MenuItem::action(t("menu.move_line_down"), editor::actions::MoveLineDown),
                MenuItem::action(t("menu.duplicate_selection"), editor::actions::DuplicateLineDown),
            ],
        },
        Menu {
            name: t("menu.view"),
            items: view_items,
        },
        Menu {
            name: t("menu.go"),
            items: vec![
                MenuItem::action(t("menu.back"), workspace::GoBack),
                MenuItem::action(t("menu.forward"), workspace::GoForward),
                MenuItem::separator(),
                MenuItem::action(t("menu.command_palette"), bspterm_actions::command_palette::Toggle),
                MenuItem::separator(),
                MenuItem::action(t("menu.go_to_file"), workspace::ToggleFileFinder::default()),
                // MenuItem::action("Go to Symbol in Project", project_symbols::Toggle),
                MenuItem::action(
                    t("menu.go_to_symbol_in_editor"),
                    bspterm_actions::outline::ToggleOutline,
                ),
                MenuItem::action(t("menu.go_to_line_column"), editor::actions::ToggleGoToLine),
                MenuItem::separator(),
                MenuItem::action(t("menu.go_to_definition"), editor::actions::GoToDefinition),
                MenuItem::action(t("menu.go_to_declaration"), editor::actions::GoToDeclaration),
                MenuItem::action(t("menu.go_to_type_definition"), editor::actions::GoToTypeDefinition),
                MenuItem::action(
                    t("menu.find_all_references"),
                    editor::actions::FindAllReferences::default(),
                ),
                MenuItem::separator(),
                MenuItem::action(t("menu.next_problem"), editor::actions::GoToDiagnostic::default()),
                MenuItem::action(
                    t("menu.previous_problem"),
                    editor::actions::GoToPreviousDiagnostic::default(),
                ),
            ],
        },
        Menu {
            name: t("menu.run"),
            items: vec![
                MenuItem::action(
                    t("menu.spawn_task"),
                    bspterm_actions::Spawn::ViaModal {
                        reveal_target: None,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(t("menu.edit_tasks_json"), crate::bspterm::OpenProjectTasks),
            ],
        },
        Menu {
            name: t("menu.window"),
            items: vec![
                MenuItem::action(t("menu.minimize"), super::Minimize),
                MenuItem::action(t("menu.zoom"), super::Zoom),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: t("menu.help"),
            items: vec![
                MenuItem::action(
                    t("menu.view_release_notes_locally"),
                    auto_update_ui::ViewReleaseNotesLocally,
                ),
                MenuItem::action(t("menu.view_telemetry"), bspterm_actions::OpenTelemetryLog),
                MenuItem::action(t("menu.view_dependency_licenses"), bspterm_actions::OpenLicenses),
                MenuItem::action(t("menu.show_welcome"), onboarding::ShowWelcome),
                MenuItem::separator(),
                MenuItem::action(t("menu.file_bug_report"), bspterm_actions::feedback::FileBugReport),
                MenuItem::action(t("menu.request_feature"), bspterm_actions::feedback::RequestFeature),
            ],
        },
    ]
}
