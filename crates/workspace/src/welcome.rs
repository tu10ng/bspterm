use crate::{
    Workspace,
    item::{Item, ItemEvent},
};
use bspterm_actions::{OpenOnboarding, rule_editor, script_panel, terminal_abbr_bar};
use gpui::WeakEntity;
use gpui::{
    Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    ParentElement, Render, Styled, Task, Window, actions,
};
use i18n::t;
use menu::{SelectNext, SelectPrevious};
use ui::{ButtonLike, Divider, DividerColor, KeyBinding, prelude::*};

actions!(
    zed,
    [
        /// Show the Zed welcome screen
        ShowWelcome
    ]
);

#[derive(IntoElement)]
struct SectionHeader {
    title: SharedString,
}

impl SectionHeader {
    fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .px_1()
            .mb_2()
            .gap_2()
            .child(
                Label::new(self.title.to_ascii_uppercase())
                    .buffer_font(cx)
                    .color(Color::Muted)
                    .size(LabelSize::XSmall),
            )
            .child(Divider::horizontal().color(DividerColor::BorderVariant))
    }
}

#[derive(IntoElement)]
struct FeatureItem {
    icon: IconName,
    label: SharedString,
    description: SharedString,
    action: Box<dyn Action>,
    tab_index: usize,
    focus_handle: FocusHandle,
}

impl FeatureItem {
    fn new(
        icon: IconName,
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
        action: &dyn Action,
        tab_index: usize,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            icon,
            label: label.into(),
            description: description.into(),
            action: action.boxed_clone(),
            tab_index,
            focus_handle,
        }
    }
}

impl RenderOnce for FeatureItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = format!("feature-{}", self.label);
        let action_ref: &dyn Action = &*self.action;

        ButtonLike::new(id)
            .tab_index(self.tab_index as isize)
            .full_width()
            .size(ButtonSize::Medium)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Icon::new(self.icon)
                                    .color(Color::Muted)
                                    .size(IconSize::Small),
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(self.label))
                                    .child(
                                        Label::new(self.description)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            ),
                    )
                    .child(
                        KeyBinding::for_action_in(action_ref, &self.focus_handle, cx)
                            .size(rems_from_px(12.)),
                    ),
            )
            .on_click(move |_, window, cx| window.dispatch_action(self.action.boxed_clone(), cx))
    }
}

#[derive(IntoElement)]
struct InfoItem {
    icon: IconName,
    label: SharedString,
    description: SharedString,
}

impl InfoItem {
    fn new(
        icon: IconName,
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            icon,
            label: label.into(),
            description: description.into(),
        }
    }
}

impl RenderOnce for InfoItem {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .child(
                Icon::new(self.icon)
                    .color(Color::Muted)
                    .size(IconSize::Small),
            )
            .child(
                v_flex()
                    .child(Label::new(self.label))
                    .child(
                        Label::new(self.description)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

pub struct WelcomePage {
    #[allow(dead_code)]
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
}

impl WelcomePage {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        _fallback_to_recent_projects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();

        WelcomePage {
            workspace,
            focus_handle,
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_next(cx);
        cx.notify();
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_prev(cx);
        cx.notify();
    }

    fn render_connect_section(&self, start_index: usize) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(t("welcome.connect")))
            .child(FeatureItem::new(
                IconName::Server,
                t("welcome.remote_explorer"),
                t("welcome.remote_explorer_desc"),
                &bspterm_actions::remote_explorer::ToggleFocus,
                start_index,
                self.focus_handle.clone(),
            ))
    }

    fn render_protocols_section(&self) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(t("welcome.protocols")))
            .child(InfoItem::new(
                IconName::LockOutlined,
                t("welcome.ssh"),
                t("welcome.ssh_desc"),
            ))
            .child(InfoItem::new(
                IconName::TerminalAlt,
                t("welcome.telnet"),
                t("welcome.telnet_desc"),
            ))
    }

    fn render_automate_section(&self, start_index: usize) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(t("welcome.automate")))
            .child(FeatureItem::new(
                IconName::BoltOutlined,
                t("welcome.automation_rules"),
                t("welcome.automation_rules_desc"),
                &rule_editor::ToggleFocus,
                start_index,
                self.focus_handle.clone(),
            ))
            .child(FeatureItem::new(
                IconName::Code,
                t("welcome.python_scripts"),
                t("welcome.python_scripts_desc"),
                &script_panel::ToggleFocus,
                start_index + 1,
                self.focus_handle.clone(),
            ))
            .child(FeatureItem::new(
                IconName::TextSnippet,
                t("welcome.abbreviations"),
                t("welcome.abbreviations_desc"),
                &terminal_abbr_bar::ConfigureAbbrBar,
                start_index + 2,
                self.focus_handle.clone(),
            ))
    }

    fn render_features_section(&self) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(t("welcome.features")))
            .child(InfoItem::new(
                IconName::ZedAgent,
                t("welcome.ai_agent"),
                t("welcome.ai_agent_desc"),
            ))
            .child(InfoItem::new(
                IconName::Send,
                t("welcome.server_sharing"),
                t("welcome.server_sharing_desc"),
            ))
            .child(InfoItem::new(
                IconName::Download,
                t("welcome.quick_import"),
                t("welcome.quick_import_desc"),
            ))
            .child(InfoItem::new(
                IconName::UserGroup,
                t("welcome.lan_discovery"),
                t("welcome.lan_discovery_desc"),
            ))
            .child(InfoItem::new(
                IconName::ArrowRightLeft,
                t("welcome.session_drag"),
                t("welcome.session_drag_desc"),
            ))
            .child(InfoItem::new(
                IconName::FileTextOutlined,
                t("welcome.export_output"),
                t("welcome.export_output_desc"),
            ))
            .child(InfoItem::new(
                IconName::Hash,
                t("welcome.line_timestamp"),
                t("welcome.line_timestamp_desc"),
            ))
            .child(InfoItem::new(
                IconName::Indicator,
                t("welcome.ping_status"),
                t("welcome.ping_status_desc"),
            ))
            .child(InfoItem::new(
                IconName::Sliders,
                t("welcome.shortcut_bar"),
                t("welcome.shortcut_bar_desc"),
            ))
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .size_full()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                h_flex()
                    .relative()
                    .size_full()
                    .px_12()
                    .max_w(px(1100.))
                    .child(
                        v_flex()
                            .flex_1()
                            .justify_center()
                            .max_w_128()
                            .mx_auto()
                            .gap_6()
                            .overflow_x_hidden()
                            .child(
                                h_flex()
                                    .w_full()
                                    .justify_center()
                                    .mb_4()
                                    .gap_4()
                                    .child(
                                        Icon::new(IconName::Terminal)
                                            .size(IconSize::Custom(rems_from_px(45.)))
                                            .color(Color::Accent),
                                    )
                                    .child(
                                        v_flex()
                                            .child(Headline::new(t("welcome.title")))
                                            .child(
                                                Label::new(t("welcome.subtitle"))
                                                    .size(LabelSize::Small)
                                                    .color(Color::Muted)
                                                    .italic(),
                                            ),
                                    ),
                            )
                            .child(self.render_connect_section(0))
                            .child(self.render_protocols_section())
                            .child(self.render_automate_section(1))
                            .child(self.render_features_section())
                            .child(
                                v_flex().gap_1().child(Divider::horizontal()).child(
                                    Button::new("welcome-exit", "Return to Onboarding")
                                        .tab_index(4_isize)
                                        .full_width()
                                        .label_size(LabelSize::XSmall)
                                        .on_click(|_, window, cx| {
                                            window.dispatch_action(
                                                OpenOnboarding.boxed_clone(),
                                                cx,
                                            );
                                        }),
                                ),
                            ),
                    ),
            )
    }
}

impl EventEmitter<ItemEvent> for WelcomePage {}

impl Focusable for WelcomePage {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for WelcomePage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Welcome".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("New Welcome Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(crate::item::ItemEvent)) {
        f(*event)
    }
}

impl crate::SerializableItem for WelcomePage {
    fn serialized_item_kind() -> &'static str {
        "WelcomePage"
    }

    fn cleanup(
        workspace_id: crate::WorkspaceId,
        alive_items: Vec<crate::ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<gpui::Result<()>> {
        crate::delete_unloaded_items(
            alive_items,
            workspace_id,
            "welcome_pages",
            &persistence::WELCOME_PAGES,
            cx,
        )
    }

    fn deserialize(
        _project: Entity<project::Project>,
        workspace: gpui::WeakEntity<Workspace>,
        workspace_id: crate::WorkspaceId,
        item_id: crate::ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<Self>>> {
        if persistence::WELCOME_PAGES
            .get_welcome_page(item_id, workspace_id)
            .ok()
            .is_some_and(|is_open| is_open)
        {
            Task::ready(Ok(
                cx.new(|cx| WelcomePage::new(workspace, false, window, cx))
            ))
        } else {
            Task::ready(Err(anyhow::anyhow!("No welcome page to deserialize")))
        }
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: crate::ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<gpui::Result<()>>> {
        let workspace_id = workspace.database_id()?;
        Some(cx.background_spawn(async move {
            persistence::WELCOME_PAGES
                .save_welcome_page(item_id, workspace_id, true)
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        event == &ItemEvent::UpdateTab
    }
}

mod persistence {
    use crate::WorkspaceDb;
    use db::{
        query,
        sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
        sqlez_macros::sql,
    };

    pub struct WelcomePagesDb(ThreadSafeConnection);

    impl Domain for WelcomePagesDb {
        const NAME: &str = stringify!(WelcomePagesDb);

        const MIGRATIONS: &[&str] = (&[sql!(
                    CREATE TABLE welcome_pages (
                        workspace_id INTEGER,
                        item_id INTEGER UNIQUE,
                        is_open INTEGER DEFAULT FALSE,

                        PRIMARY KEY(workspace_id, item_id),
                        FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                        ON DELETE CASCADE
                    ) STRICT;
        )]);
    }

    db::static_connection!(WELCOME_PAGES, WelcomePagesDb, [WorkspaceDb]);

    impl WelcomePagesDb {
        query! {
            pub async fn save_welcome_page(
                item_id: crate::ItemId,
                workspace_id: crate::WorkspaceId,
                is_open: bool
            ) -> Result<()> {
                INSERT OR REPLACE INTO welcome_pages(item_id, workspace_id, is_open)
                VALUES (?, ?, ?)
            }
        }

        query! {
            pub fn get_welcome_page(
                item_id: crate::ItemId,
                workspace_id: crate::WorkspaceId
            ) -> Result<bool> {
                SELECT is_open
                FROM welcome_pages
                WHERE item_id = ? AND workspace_id = ?
            }
        }
    }
}
