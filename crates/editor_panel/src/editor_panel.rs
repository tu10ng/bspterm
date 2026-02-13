use std::sync::Arc;

use anyhow::Result;
use collections::HashMap;
use db::kvp::KEY_VALUE_STORE;
use editor::Editor;
use gpui::{
    Action, App, AsyncWindowContext, Context, Corner, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Pixels, Render, Styled, Task, WeakEntity, Window,
    actions, px,
};
use project::{Fs, Project};
use serde::{Deserialize, Serialize};
use ui::{
    prelude::*, ContextMenu, DynamicSpacing, IconButton, IconName, IconSize, PopoverMenu, Tooltip,
};
use util::ResultExt;
use workspace::{
    ActivateNextPane, ActivatePane, ActivatePaneDown, ActivatePaneLeft, ActivatePaneRight,
    ActivatePaneUp, ActivatePreviousPane, MoveItemToPane, MoveItemToPaneInDirection, MovePaneDown,
    MovePaneLeft, MovePaneRight, MovePaneUp, NewFile, Pane, PaneGroup, SplitDirection, SplitDown,
    SplitLeft, SplitMode, SplitRight, SplitUp, SwapPaneDown, SwapPaneLeft, SwapPaneRight,
    SwapPaneUp, ToggleFileFinder, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    move_active_item, pane,
};

const EDITOR_PANEL_KEY: &str = "EditorPanel";

actions!(
    editor_panel,
    [
        /// Toggles the editor panel.
        Toggle,
        /// Toggles focus on the editor panel.
        ToggleFocus,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _cx: &mut Context<Workspace>| {
            workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
                workspace.toggle_panel_focus::<EditorPanel>(window, cx);
            });
            workspace.register_action(|workspace, _: &Toggle, window, cx| {
                if !workspace.toggle_panel_focus::<EditorPanel>(window, cx) {
                    workspace.close_panel::<EditorPanel>(window, cx);
                }
            });
        },
    )
    .detach();
}

pub struct EditorPanel {
    pub(crate) active_pane: Entity<Pane>,
    pub(crate) center: PaneGroup,
    #[allow(dead_code)]
    fs: Arc<dyn Fs>,
    workspace: WeakEntity<Workspace>,
    pub(crate) width: Option<Pixels>,
    pub(crate) height: Option<Pixels>,
    pending_serialization: Task<Option<()>>,
    active: bool,
}

#[derive(Serialize, Deserialize)]
struct SerializedEditorPanel {
    width: Option<Pixels>,
    height: Option<Pixels>,
}

impl EditorPanel {
    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = workspace.project();
        let pane = new_editor_pane(workspace.weak_handle(), project.clone(), window, cx);
        let center = PaneGroup::new(pane.clone());
        let editor_panel = Self {
            center,
            active_pane: pane,
            fs: workspace.app_state().fs.clone(),
            workspace: workspace.weak_handle(),
            pending_serialization: Task::ready(None),
            width: None,
            height: None,
            active: false,
        };
        editor_panel.apply_tab_bar_buttons(&editor_panel.active_pane, cx);
        editor_panel
    }

    pub(crate) fn apply_tab_bar_buttons(&self, pane: &Entity<Pane>, cx: &mut Context<Self>) {
        pane.update(cx, |pane, cx| {
            pane.set_render_tab_bar_buttons(cx, move |pane, window, cx| {
                if !pane.has_focus(window, cx) && !pane.context_menu_focused(window, cx) {
                    return (None, None);
                }
                let focus_handle = pane.focus_handle(cx);
                let (can_clone, can_split_move) = match pane.active_item() {
                    Some(active_item) if active_item.can_split(cx) => (true, false),
                    Some(_) => (false, pane.items_len() > 1),
                    None => (false, false),
                };
                let right_children = h_flex()
                    .gap(DynamicSpacing::Base02.rems(cx))
                    .child(
                        PopoverMenu::new("editor-tab-bar-popover-menu")
                            .trigger_with_tooltip(
                                IconButton::new("plus", IconName::Plus).icon_size(IconSize::Small),
                                Tooltip::text("New..."),
                            )
                            .anchor(Corner::TopRight)
                            .with_handle(pane.new_item_context_menu_handle.clone())
                            .menu(move |window, cx| {
                                let focus_handle = focus_handle.clone();
                                let menu = ContextMenu::build(window, cx, |menu, _, _| {
                                    menu.context(focus_handle.clone())
                                        .action("New File", NewFile.boxed_clone())
                                        .action(
                                            "Open File",
                                            ToggleFileFinder::default().boxed_clone(),
                                        )
                                });
                                Some(menu)
                            }),
                    )
                    .child(
                        PopoverMenu::new("editor-pane-tab-bar-split")
                            .trigger_with_tooltip(
                                IconButton::new("editor-pane-split", IconName::Split)
                                    .icon_size(IconSize::Small)
                                    .disabled(!can_clone && !can_split_move),
                                Tooltip::text("Split Pane"),
                            )
                            .anchor(Corner::TopRight)
                            .with_handle(pane.split_item_context_menu_handle.clone())
                            .menu(move |window, cx| {
                                ContextMenu::build(window, cx, |menu, _, _| {
                                    let mode = SplitMode::MovePane;
                                    if can_split_move {
                                        menu.action("Split Right", SplitRight { mode }.boxed_clone())
                                            .action("Split Left", SplitLeft { mode }.boxed_clone())
                                            .action("Split Up", SplitUp { mode }.boxed_clone())
                                            .action("Split Down", SplitDown { mode }.boxed_clone())
                                    } else {
                                        menu.action(
                                            "Split Right",
                                            SplitRight::default().boxed_clone(),
                                        )
                                        .action("Split Left", SplitLeft::default().boxed_clone())
                                        .action("Split Up", SplitUp::default().boxed_clone())
                                        .action("Split Down", SplitDown::default().boxed_clone())
                                    }
                                })
                                .into()
                            }),
                    )
                    .child({
                        let zoomed = pane.is_zoomed();
                        IconButton::new("toggle_zoom", IconName::Maximize)
                            .icon_size(IconSize::Small)
                            .toggle_state(zoomed)
                            .selected_icon(IconName::Minimize)
                            .on_click(cx.listener(|pane, _, window, cx| {
                                pane.toggle_zoom(&workspace::ToggleZoom, window, cx);
                            }))
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action(
                                    if zoomed { "Zoom Out" } else { "Zoom In" },
                                    &workspace::ToggleZoom,
                                    cx,
                                )
                            })
                    })
                    .into_any_element()
                    .into();
                (None, right_children)
            });
        });
    }

    fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            pane::Event::ActivateItem { .. } => self.serialize(cx),
            pane::Event::RemovedItem { .. } => self.serialize(cx),
            pane::Event::Remove { focus_on_pane } => {
                let pane_count_before_removal = self.center.panes().len();
                let _removal_result = self.center.remove(pane, cx);
                if pane_count_before_removal == 1 {
                    self.center.first_pane().update(cx, |pane, cx| {
                        pane.set_zoomed(false, cx);
                    });
                    cx.emit(PanelEvent::Close);
                } else if let Some(focus_on_pane) =
                    focus_on_pane.as_ref().or_else(|| self.center.panes().pop())
                {
                    focus_on_pane.focus_handle(cx).focus(window, cx);
                }
            }
            pane::Event::ZoomIn => {
                for pane in self.center.panes() {
                    pane.update(cx, |pane, cx| {
                        pane.set_zoomed(true, cx);
                    })
                }
                cx.emit(PanelEvent::ZoomIn);
                cx.notify();
            }
            pane::Event::ZoomOut => {
                for pane in self.center.panes() {
                    pane.update(cx, |pane, cx| {
                        pane.set_zoomed(false, cx);
                    })
                }
                cx.emit(PanelEvent::ZoomOut);
                cx.notify();
            }
            pane::Event::AddItem { item } => {
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        item.added_to_pane(workspace, pane.clone(), window, cx)
                    })
                }
                self.serialize(cx);
            }
            &pane::Event::Split { direction, mode } => {
                let Ok(project) = self
                    .workspace
                    .update(cx, |workspace, _| workspace.project().clone())
                else {
                    return;
                };
                match mode {
                    SplitMode::ClonePane | SplitMode::EmptyPane => {
                        let new_pane =
                            new_editor_pane(self.workspace.clone(), project, window, cx);
                        self.apply_tab_bar_buttons(&new_pane, cx);
                        self.center.split(pane, &new_pane, direction, cx).log_err();
                        window.focus(&new_pane.focus_handle(cx), cx);
                    }
                    SplitMode::MovePane => {
                        let Some(item) =
                            pane.update(cx, |pane, cx| pane.take_active_item(window, cx))
                        else {
                            return;
                        };
                        let new_pane =
                            new_editor_pane(self.workspace.clone(), project, window, cx);
                        self.apply_tab_bar_buttons(&new_pane, cx);
                        new_pane.update(cx, |pane, cx| {
                            pane.add_item(item, true, true, None, window, cx);
                        });
                        self.center.split(pane, &new_pane, direction, cx).log_err();
                        window.focus(&new_pane.focus_handle(cx), cx);
                    }
                };
            }
            pane::Event::Focus => {
                self.active_pane = pane.clone();
            }
            _ => {}
        }
        self.serialize(cx);
    }

    fn serialize(&mut self, cx: &mut Context<Self>) {
        let width = self.width;
        let height = self.height;
        self.pending_serialization = cx.background_executor().spawn(async move {
            KEY_VALUE_STORE
                .write_kvp(
                    EDITOR_PANEL_KEY.into(),
                    serde_json::to_string(&SerializedEditorPanel { width, height })
                        .ok()
                        .unwrap_or_default(),
                )
                .await
                .ok();
            Some(())
        });
    }

    fn has_no_items(&self, cx: &App) -> bool {
        self.center
            .panes()
            .iter()
            .all(|pane| pane.read(cx).items_len() == 0)
    }

    fn activate_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
        {
            window.focus(&pane.focus_handle(cx), cx);
        }
    }

    fn swap_pane_in_direction(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        if let Some(to) = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
            .cloned()
        {
            self.center.swap(&self.active_pane.clone(), &to, cx);
            cx.notify();
        }
    }

    fn move_pane_to_border(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        self.center
            .move_to_border(&self.active_pane, direction, cx)
            .log_err();
        cx.notify();
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        let serialized_panel = cx
            .background_executor()
            .spawn(async move {
                KEY_VALUE_STORE
                    .read_kvp(EDITOR_PANEL_KEY)
                    .log_err()
                    .flatten()
                    .and_then(|value| serde_json::from_str::<SerializedEditorPanel>(&value).ok())
            })
            .await;

        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| {
                let mut editor_panel = EditorPanel::new(workspace, window, cx);
                if let Some(serialized_panel) = serialized_panel {
                    editor_panel.width = serialized_panel.width;
                    editor_panel.height = serialized_panel.height;
                }
                editor_panel
            })
        })
    }
}

fn new_editor_pane(
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut Context<EditorPanel>,
) -> Entity<Pane> {
    let pane = cx.new(|cx| {
        let mut pane = Pane::new(
            workspace.clone(),
            project.clone(),
            Default::default(),
            None,
            NewFile.boxed_clone(),
            false,
            window,
            cx,
        );
        pane.set_can_split(Some(Arc::new(|_, _, _, _| true)));
        pane.set_should_display_tab_bar(|_, _| true);
        pane.set_close_pane_if_empty(true, cx);
        pane
    });

    cx.subscribe_in(&pane, window, EditorPanel::handle_pane_event)
        .detach();
    cx.observe(&pane, |_, _, cx| cx.notify()).detach();

    pane
}

impl EventEmitter<PanelEvent> for EditorPanel {}

impl Render for EditorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace
            .update(cx, |workspace, cx| {
                div().size_full().child(self.center.render(
                    workspace.zoomed_item(),
                    &workspace::PaneRenderContext {
                        follower_states: &HashMap::default(),
                        active_call: workspace.active_call(),
                        active_pane: &self.active_pane,
                        app_state: workspace.app_state(),
                        project: workspace.project(),
                        workspace: &workspace.weak_handle(),
                    },
                    window,
                    cx,
                ))
            })
            .ok()
            .map(|div| {
                div.on_action(
                    cx.listener(|editor_panel, _: &ActivatePaneLeft, window, cx| {
                        editor_panel.activate_pane_in_direction(SplitDirection::Left, window, cx);
                    }),
                )
                .on_action(
                    cx.listener(|editor_panel, _: &ActivatePaneRight, window, cx| {
                        editor_panel.activate_pane_in_direction(SplitDirection::Right, window, cx);
                    }),
                )
                .on_action(cx.listener(|editor_panel, _: &ActivatePaneUp, window, cx| {
                    editor_panel.activate_pane_in_direction(SplitDirection::Up, window, cx);
                }))
                .on_action(
                    cx.listener(|editor_panel, _: &ActivatePaneDown, window, cx| {
                        editor_panel.activate_pane_in_direction(SplitDirection::Down, window, cx);
                    }),
                )
                .on_action(
                    cx.listener(|editor_panel, _action: &ActivateNextPane, window, cx| {
                        let panes = editor_panel.center.panes();
                        if let Some(ix) = panes
                            .iter()
                            .position(|pane| **pane == editor_panel.active_pane)
                        {
                            let next_ix = (ix + 1) % panes.len();
                            window.focus(&panes[next_ix].focus_handle(cx), cx);
                        }
                    }),
                )
                .on_action(cx.listener(
                    |editor_panel, _action: &ActivatePreviousPane, window, cx| {
                        let panes = editor_panel.center.panes();
                        if let Some(ix) = panes
                            .iter()
                            .position(|pane| **pane == editor_panel.active_pane)
                        {
                            let prev_ix = std::cmp::min(ix.wrapping_sub(1), panes.len() - 1);
                            window.focus(&panes[prev_ix].focus_handle(cx), cx);
                        }
                    },
                ))
                .on_action(
                    cx.listener(|editor_panel, action: &ActivatePane, window, cx| {
                        let panes = editor_panel.center.panes();
                        if let Some(&pane) = panes.get(action.0) {
                            window.focus(&pane.read(cx).focus_handle(cx), cx);
                        }
                    }),
                )
                .on_action(cx.listener(|editor_panel, _: &SwapPaneLeft, _, cx| {
                    editor_panel.swap_pane_in_direction(SplitDirection::Left, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &SwapPaneRight, _, cx| {
                    editor_panel.swap_pane_in_direction(SplitDirection::Right, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &SwapPaneUp, _, cx| {
                    editor_panel.swap_pane_in_direction(SplitDirection::Up, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &SwapPaneDown, _, cx| {
                    editor_panel.swap_pane_in_direction(SplitDirection::Down, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &MovePaneLeft, _, cx| {
                    editor_panel.move_pane_to_border(SplitDirection::Left, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &MovePaneRight, _, cx| {
                    editor_panel.move_pane_to_border(SplitDirection::Right, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &MovePaneUp, _, cx| {
                    editor_panel.move_pane_to_border(SplitDirection::Up, cx);
                }))
                .on_action(cx.listener(|editor_panel, _: &MovePaneDown, _, cx| {
                    editor_panel.move_pane_to_border(SplitDirection::Down, cx);
                }))
                .on_action(
                    cx.listener(|editor_panel, action: &MoveItemToPane, window, cx| {
                        let Some(&target_pane) =
                            editor_panel.center.panes().get(action.destination)
                        else {
                            return;
                        };
                        move_active_item(
                            &editor_panel.active_pane,
                            target_pane,
                            action.focus,
                            true,
                            window,
                            cx,
                        );
                    }),
                )
                .on_action(cx.listener(
                    |editor_panel, action: &MoveItemToPaneInDirection, window, cx| {
                        let source_pane = &editor_panel.active_pane;
                        if let Some(destination_pane) = editor_panel
                            .center
                            .find_pane_in_direction(source_pane, action.direction, cx)
                        {
                            move_active_item(
                                source_pane,
                                destination_pane,
                                action.focus,
                                true,
                                window,
                                cx,
                            );
                        };
                    },
                ))
            })
            .unwrap_or_else(|| div())
    }
}

impl Focusable for EditorPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.active_pane.focus_handle(cx)
    }
}

impl Panel for EditorPanel {
    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Bottom
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Bottom)
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, window: &Window, cx: &App) -> Pixels {
        match self.position(window, cx) {
            DockPosition::Left | DockPosition::Right => self.width.unwrap_or(px(400.0)),
            DockPosition::Bottom => self.height.unwrap_or(px(300.0)),
        }
    }

    fn set_size(&mut self, size: Option<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        match self.position(window, cx) {
            DockPosition::Left | DockPosition::Right => self.width = size,
            DockPosition::Bottom => self.height = size,
        }
        cx.notify();
        cx.defer_in(window, |this, _, cx| {
            this.serialize(cx);
        })
    }

    fn is_zoomed(&self, _window: &Window, cx: &App) -> bool {
        self.active_pane.read(cx).is_zoomed()
    }

    fn set_zoomed(&mut self, zoomed: bool, _: &mut Window, cx: &mut Context<Self>) {
        for pane in self.center.panes() {
            pane.update(cx, |pane, cx| {
                pane.set_zoomed(zoomed, cx);
            })
        }
        cx.notify();
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        let old_active = self.active;
        self.active = active;
        if !active || old_active == active || !self.has_no_items(cx) {
            return;
        }
        cx.defer_in(window, |this, window, cx| {
            if let Some(workspace) = this.workspace.upgrade() {
                workspace.update(cx, |workspace, cx| {
                    let project = workspace.project().clone();
                    let buffer = project.update(cx, |project, cx| {
                        project.create_local_buffer("", None, true, cx)
                    });
                    let editor = cx.new(|cx| {
                        Editor::for_buffer(buffer, Some(project), window, cx)
                    });
                    this.active_pane.update(cx, |pane, cx| {
                        pane.add_item(Box::new(editor), true, true, None, window, cx);
                    });
                });
            }
        });
    }

    fn icon_label(&self, _window: &Window, cx: &App) -> Option<String> {
        let count = self
            .center
            .panes()
            .into_iter()
            .map(|pane| pane.read(cx).items_len())
            .sum::<usize>();
        if count == 0 {
            None
        } else {
            Some(count.to_string())
        }
    }

    fn persistent_name() -> &'static str {
        "EditorPanel"
    }

    fn panel_key() -> &'static str {
        EDITOR_PANEL_KEY
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Code)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Editor Panel")
    }

    fn activation_priority(&self) -> u32 {
        5
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _window: &Window, _cx: &App) -> bool {
        false
    }
}
