use std::ops::Range;

use anyhow::Result;
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, ParentElement, Render, Styled, Subscription, UniformListScrollHandle, WeakEntity,
    Window, px, uniform_list,
};
use terminal::{SessionNode, SessionStoreEntity, SessionStoreEvent};
use ui::{prelude::*, Color, Icon, IconName, IconSize, Label, ListItem, ListItemSpacing, v_flex};
use uuid::Uuid;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};
use zed_actions::remote_explorer::ToggleFocus;

const REMOTE_EXPLORER_PANEL_KEY: &str = "RemoteExplorerPanel";

pub fn init(cx: &mut App) {
    SessionStoreEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<RemoteExplorer>(window, cx);
        });
    })
    .detach();
}

/// A flattened tree entry for uniform list rendering.
#[derive(Clone, Debug)]
pub struct FlattenedEntry {
    pub id: Uuid,
    pub depth: usize,
    pub node: SessionNode,
}

pub struct RemoteExplorer {
    session_store: Entity<SessionStoreEntity>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    visible_entries: Vec<FlattenedEntry>,
    #[allow(dead_code)]
    workspace: WeakEntity<Workspace>,
    width: Option<Pixels>,
    _subscription: Subscription,
}

impl RemoteExplorer {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| Self::new(workspace, window, cx))
        })
    }

    pub fn new(
        workspace: &Workspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_store = SessionStoreEntity::global(cx);
        let focus_handle = cx.focus_handle();

        let subscription = cx.subscribe(&session_store, |this, _, event, cx| {
            match event {
                SessionStoreEvent::Changed
                | SessionStoreEvent::SessionAdded(_)
                | SessionStoreEvent::SessionRemoved(_) => {
                    this.update_visible_entries(cx);
                }
            }
        });

        let mut this = Self {
            session_store,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            visible_entries: Vec::new(),
            workspace: workspace.weak_handle(),
            width: None,
            _subscription: subscription,
        };

        this.update_visible_entries(cx);
        this
    }

    fn update_visible_entries(&mut self, cx: &mut Context<Self>) {
        let session_store = self.session_store.read(cx);
        let store = session_store.store();

        let mut entries = Vec::new();
        Self::flatten_nodes(&store.root, 0, &mut entries);
        self.visible_entries = entries;
        cx.notify();
    }

    fn flatten_nodes(
        nodes: &[SessionNode],
        depth: usize,
        result: &mut Vec<FlattenedEntry>,
    ) {
        for node in nodes {
            result.push(FlattenedEntry {
                id: node.id(),
                depth,
                node: node.clone(),
            });

            if let SessionNode::Group(group) = node {
                if group.expanded {
                    Self::flatten_nodes(&group.children, depth + 1, result);
                }
            }
        }
    }

    fn toggle_expanded(&mut self, id: Uuid, _window: &mut Window, cx: &mut Context<Self>) {
        self.session_store.update(cx, |store, cx| {
            store.toggle_group_expanded(id, cx);
        });
        self.update_visible_entries(cx);
    }

    fn render_entry(
        &self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ListItem {
        let entry = &self.visible_entries[index];
        let id = entry.id;
        let depth = entry.depth;

        let (icon, name, is_group, is_expanded) = match &entry.node {
            SessionNode::Group(group) => (
                if group.expanded {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                },
                group.name.clone(),
                true,
                Some(group.expanded),
            ),
            SessionNode::Session(session) => (
                IconName::Server,
                session.name.clone(),
                false,
                None,
            ),
        };

        ListItem::new(id)
            .indent_level(depth)
            .indent_step_size(px(12.))
            .spacing(ListItemSpacing::Dense)
            .toggle(is_expanded)
            .when(is_group, |this| {
                this.on_toggle(cx.listener(move |this, _, window, cx| {
                    this.toggle_expanded(id, window, cx);
                }))
            })
            .start_slot(
                Icon::new(icon)
                    .color(Color::Muted)
                    .size(IconSize::Small),
            )
            .child(Label::new(name))
    }

    fn render_entries(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<ListItem> {
        let mut items = Vec::with_capacity(range.len());
        for ix in range {
            items.push(self.render_entry(ix, window, cx));
        }
        items
    }
}

impl EventEmitter<PanelEvent> for RemoteExplorer {}

impl Render for RemoteExplorer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let item_count = self.visible_entries.len();

        v_flex()
            .id("remote-explorer")
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .child(if item_count > 0 {
                uniform_list(
                    "remote-explorer-list",
                    item_count,
                    cx.processor(|this, range: Range<usize>, window, cx| {
                        this.render_entries(range, window, cx)
                    }),
                )
                .flex_1()
                .track_scroll(&self.scroll_handle)
                .into_any_element()
            } else {
                v_flex()
                    .p_4()
                    .gap_2()
                    .child(
                        Label::new("No saved sessions")
                            .color(Color::Muted)
                    )
                    .into_any_element()
            })
    }
}

impl Focusable for RemoteExplorer {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for RemoteExplorer {
    fn persistent_name() -> &'static str {
        "Remote Explorer"
    }

    fn panel_key() -> &'static str {
        REMOTE_EXPLORER_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, _position: DockPosition, _window: &mut Window, _cx: &mut Context<Self>) {
    }

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(240.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Server)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Remote Explorer")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        10
    }
}
