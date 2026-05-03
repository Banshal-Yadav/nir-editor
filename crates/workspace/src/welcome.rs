use crate::{
    NewFile, Open, OpenMode, PathList, SerializedWorkspaceLocation, ToggleWorkspaceSidebar,
    Workspace, WorkspaceId,
    item::{Item, ItemEvent},
    persistence::WorkspaceDb,
};
use agent_settings::AgentSettings;
use chrono::{DateTime, Utc};
use std::time::Duration;
use gpui::{
    px, rgba, App, AppContext, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    Entity, Render, WeakEntity, point,
    Action, actions, Task, Window, Animation, AnimationExt, pulsating_between,
};
use menu::{SelectNext, SelectPrevious};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings;
use ui::{ButtonLike, KeyBinding, prelude::*};
use util::ResultExt;
use zed_actions::{
    OpenKeymap, OpenSettings, assistant::ToggleFocus,
};

#[derive(PartialEq, Clone, Debug, Deserialize, Serialize, JsonSchema, Action)]
#[action(namespace = welcome)]
#[serde(transparent)]
pub struct OpenRecentProject {
    pub index: usize,
}

actions!(
    zed,
    [
        /// Show the /void welcome screen
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
            .px_2()
            .py_0p5()
            .mb_2()
            .bg(cx.theme().colors().border_variant)
            .child(
                Label::new(self.title.to_string())
                    .color(Color::Default)
                    .weight(FontWeight::EXTRA_BOLD)
                    .size(LabelSize::XSmall),
            )
    }
}

#[derive(IntoElement)]
struct SectionButton {
    label: SharedString,
    description: SharedString,
    #[allow(dead_code)]
    icon: IconName,
    action: Box<dyn Action>,
    tab_index: usize,
    focus_handle: FocusHandle,
}

impl SectionButton {
    fn new(
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
        icon: IconName,
        action: &dyn Action,
        tab_index: usize,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
            icon,
            action: action.boxed_clone(),
            tab_index,
            focus_handle,
        }
    }
}

impl RenderOnce for SectionButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = format!("onb-button-{}-{}", self.label, self.tab_index);
        let action_ref: &dyn Action = &*self.action;
        let _colors = cx.theme().colors();

        ButtonLike::new(id)
            .tab_index(self.tab_index as isize)
            .full_width()
            .child(
                v_flex()
                    .w_full()
                    .p_4()
                    .gap_2()
                    .child(
                        Label::new(self.label.to_ascii_uppercase())
                            .weight(FontWeight::EXTRA_BOLD)
                            .size(LabelSize::Small),
                    )
                    .child(
                        Label::new(self.description.clone())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .px_2()
                            .py_1()
                            .child(
                                KeyBinding::for_action_in(action_ref, &self.focus_handle, cx),
                            ),
                    ),
            )
            .on_click(move |_, window, cx| {
                self.focus_handle.dispatch_action(&*self.action, window, cx)
            })
    }
}

/// Custom /void logo component with SVG and blinking cursor
#[derive(IntoElement)]
struct VoidLogo {
    #[allow(dead_code)]
    phantom: std::marker::PhantomData<()>,
}

impl VoidLogo {
    fn new(_cx: &App) -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }
}

impl RenderOnce for VoidLogo {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let cursor_animation = Animation::new(std::time::Duration::from_secs(1))
            .repeat()
            .with_easing(pulsating_between(0.3, 1.0));

        h_flex()
            .items_center()
            .child(
                Label::new("[/]")
                    .weight(FontWeight::EXTRA_BOLD)
                    .size(LabelSize::XLarge)
                    .color(Color::Accent)
            )
            .child(
                Label::new("void")
                    .weight(FontWeight::EXTRA_BOLD)
                    .size(LabelSize::XLarge)
            )
            .child(
                Label::new("▊")
                    .size(LabelSize::XLarge)
                    .color(Color::Accent)
                    .with_animation("void-cursor", cursor_animation, |label, delta| {
                        label.opacity(delta)
                    }),
            )
    }
}

enum SectionVisibility {
    Always,
}

impl SectionVisibility {
    fn is_visible(&self) -> bool {
        match self {
            SectionVisibility::Always => true,
        }
    }
}

struct SectionEntry {
    icon: IconName,
    title: &'static str,
    description: &'static str,
    action: &'static dyn Action,
    visibility_guard: SectionVisibility,
}

impl SectionEntry {
    fn render(&self, button_index: usize, focus: &FocusHandle) -> Option<impl IntoElement> {
        self.visibility_guard.is_visible().then(|| {
            SectionButton::new(
                self.title,
                self.description,
                self.icon,
                self.action,
                button_index,
                focus.clone(),
            )
        })
    }
}

const CONTENT: (Section<2>, Section<2>) = (
    Section {
        title: "Get Started",
        entries: [
            SectionEntry {
                icon: IconName::Plus,
                title: "New File",
                description: "Initialize empty buffer",
                action: &NewFile,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::FolderOpen,
                title: "Open Project",
                description: "Load workspace from disk",
                action: &Open::DEFAULT,
                visibility_guard: SectionVisibility::Always,
            },
        ],
    },
    Section {
        title: "Configuration",
        entries: [
            SectionEntry {
                icon: IconName::Settings,
                title: "Settings",
                description: "Configure system prefs",
                action: &OpenSettings,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::Keyboard,
                title: "Keymaps",
                description: "Modify input bindings",
                action: &OpenKeymap,
                visibility_guard: SectionVisibility::Always,
            },
        ],
    },
);

struct Section<const COLS: usize> {
    title: &'static str,
    entries: [SectionEntry; COLS],
}

impl<const COLS: usize> Section<COLS> {
    fn render(self, index_offset: usize, focus: &FocusHandle, cx: &mut App) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(self.title))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_px()
                    .bg(cx.theme().colors().background)
                    .border_3()
                    .border_color(cx.theme().colors().border)
                    .children(
                        self.entries
                            .iter()
                            .enumerate()
                            .filter_map(|(index, entry)| entry.render(index_offset + index, focus)),
                    ),
            )
    }
}

pub struct WelcomePage {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    fallback_to_recent_projects: bool,
    recent_workspaces: Option<
        Vec<(
            WorkspaceId,
            SerializedWorkspaceLocation,
            PathList,
            DateTime<Utc>,
        )>,
    >,
}

impl WelcomePage {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        fallback_to_recent_projects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();

        if fallback_to_recent_projects {
            let fs = workspace
                .upgrade()
                .map(|ws| ws.read(cx).app_state().fs.clone());
            let db = WorkspaceDb::global(cx);
            cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
                let Some(fs) = fs else { return };
                let workspaces = db
                    .recent_project_workspaces(fs.as_ref())
                    .await
                    .log_err()
                    .unwrap_or_default();

                this.update(cx, |this, cx| {
                    this.recent_workspaces = Some(workspaces);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }

        WelcomePage {
            workspace,
            focus_handle,
            fallback_to_recent_projects,
            recent_workspaces: None,
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

    fn open_recent_project(
        &mut self,
        action: &OpenRecentProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(recent_workspaces) = &self.recent_workspaces {
            if let Some((_workspace_id, location, paths, _timestamp)) =
                recent_workspaces.get(action.index)
            {
                let is_local = matches!(location, SerializedWorkspaceLocation::Local);

                if is_local {
                    let paths = paths.clone();
                    let paths = paths.paths().to_vec();
                    self.workspace
                        .update(cx, |workspace, cx| {
                            workspace
                                .open_workspace_for_paths(OpenMode::Activate, paths, window, cx)
                                .detach_and_log_err(cx);
                        })
                        .log_err();
                } else {
                    use zed_actions::OpenRecent;
                    window.dispatch_action(OpenRecent::default().boxed_clone(), cx);
                }
            }
        }
    }

    fn render_agent_card(&self, tab_index: usize, cx: &mut App) -> impl IntoElement {
        let _color = cx.theme().colors();
        let description = "AGENT PROTOCOL: ACTIVATE. THINK. BUILD. SHIP.";
        let focus = self.focus_handle.clone();
        let _accent_color = cx.theme().colors().border_focused;

        v_flex()
            .w_full()
            .p_8()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("AGENT_PROTOCOL_ENABLED")
                    .weight(FontWeight::EXTRA_BOLD)
                    .size(LabelSize::Small)
                    .mb_4(),
            )
            .child(
                Label::new("Deploy parallel threads to solve complex tasks. Multi-agent orchestration and automated worktree isolation are active.")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .mb_6(),
            )
            .child(
                ButtonLike::new("open-agent")
                    .tab_index(tab_index as isize)
                    .on_click(move |_, window, cx| {
                        focus.dispatch_action(&ToggleWorkspaceSidebar, window, cx);
                        focus.dispatch_action(&ToggleFocus, window, cx);
                    })
                    .child(
                        div()
                            .bg(rgba(0xffb000ff)) // Exact Void Amber
                            .px_6()
                            .py_3()
                            .shadow(vec![gpui::BoxShadow {
                                color: rgba(0x000000ff).into(),
                                offset: point(px(6.), px(6.)),
                                blur_radius: px(0.),
                                spread_radius: px(0.),
                            }])
                            .child(
                                Label::new("OPEN AGENT PANEL")
                                    .weight(FontWeight::EXTRA_BOLD)
                                    .size(LabelSize::Small)
                                    .color(gpui::black()),
                            ),
                    ),
            )
    }

    fn render_recent_project_section(
        &self,
        recent_projects: Vec<impl IntoElement>,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .child(SectionHeader::new("Recent Projects"))
            .children(recent_projects)
    }

    fn render_recent_project(
        &self,
        project_index: usize,
        tab_index: usize,
        location: &SerializedWorkspaceLocation,
        paths: &PathList,
    ) -> impl IntoElement {
        let name = project_name(paths);

        let (icon, title) = match location {
            SerializedWorkspaceLocation::Local => (IconName::Folder, name),
            SerializedWorkspaceLocation::Remote(_) => (IconName::Server, name),
        };

        SectionButton::new(
            title,
            icon,
            &OpenRecentProject {
                index: project_index,
            },
            tab_index,
            self.focus_handle.clone(),
        )
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (first_section, second_section) = CONTENT;
        let first_section_entries = first_section.entries.len();
        let mut next_tab_index = first_section_entries + second_section.entries.len();


        let recent_projects = self
            .recent_workspaces
            .as_ref()
            .into_iter()
            .flatten()
            .take(5)
            .enumerate()
            .map(|(index, (_, loc, paths, _))| {
                self.render_recent_project(index, first_section_entries + index, loc, paths)
            })
            .collect::<Vec<_>>();

        let showing_recent_projects =
            self.fallback_to_recent_projects && !recent_projects.is_empty();
        let rendered_second_section = if showing_recent_projects {
            Some(self.render_recent_project_section(recent_projects).into_any_element())
        } else {
            None
        };

        h_flex()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::open_recent_project))
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .justify_center()
            .child(
                div()
                    .id("welcome-container")
                    .mt_20()
                    .w(rems(36.))
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .shadow(vec![gpui::BoxShadow {
                        color: rgba(0x000000ff).into(),
                        offset: point(px(20.), px(20.)),
                        blur_radius: px(0.),
                        spread_radius: px(0.),
                    }])
                    .child(
                        v_flex()
                            .child(
                                h_flex()
                                    .w_full()
                                    .h(rems(12.))
                                    .border_b_1()
                                    .border_color(cx.theme().colors().border)
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .px_8()
                                            .justify_between()
                                            .items_center()
                                            .child(VoidLogo::new(cx))
                                    .child(
                                        div()
                                            .pr_12()
                                            .child(
                                                Label::new("THINK. BUILD. SHIP.")
                                                    .weight(FontWeight::EXTRA_BOLD)
                                                    .size(LabelSize::Small)
                                                    .color(Color::Accent),
                                            )
                                    ),
                                    )
                            )
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .child(
                                        div()
                                            .border_r_1()
                                            .border_b_1()
                                            .border_color(cx.theme().colors().border)
                                            .child(first_section.entries[0].render(0, &self.focus_handle).unwrap())
                                    )
                                    .child(
                                        div()
                                            .border_b_1()
                                            .border_color(cx.theme().colors().border)
                                            .child(first_section.entries[1].render(1, &self.focus_handle).unwrap())
                                    )
                                    .when_some(rendered_second_section, |this, section| {
                                        this.child(
                                            div()
                                                .col_span(2)
                                                .border_b_1()
                                                .border_color(cx.theme().colors().border)
                                                .child(section)
                                        )
                                    })
                                    .when(rendered_second_section.is_none(), |this| {
                                        this.child(
                                            div()
                                                .border_r_1()
                                                .border_b_1()
                                                .border_color(cx.theme().colors().border)
                                                .child(second_section.entries[0].render(2, &self.focus_handle).unwrap())
                                        )
                                        .child(
                                            div()
                                                .border_b_1()
                                                .border_color(cx.theme().colors().border)
                                                .child(second_section.entries[1].render(3, &self.focus_handle).unwrap())
                                        )
                                    })
                            )
                            .child(
                                div()
                                    .p_8()
                                    .child(self.render_agent_card(next_tab_index, cx))
                            )
                            .child(
                                div()
                                    .border_t_1()
                                    .border_color(cx.theme().colors().border)
                                    .grid()
                                    .grid_cols(3)
                                    .child(
                                        div()
                                            .border_r_1()
                                            .border_color(cx.theme().colors().border)
                                            .p_4()
                                            .w_full()
                                            .flex()
                                            .justify_center()
                                            .child(
                                                Label::new("CLONE_REPO")
                                                    .size(LabelSize::XSmall)
                                            )
                                    )
                                    .child(
                                        div()
                                            .border_r_1()
                                            .border_color(cx.theme().colors().border)
                                            .p_4()
                                            .w_full()
                                            .flex()
                                            .justify_center()
                                            .child(
                                                Label::new("COMMAND_PALETTE")
                                                    .size(LabelSize::XSmall)
                                            )
                                    )
                                    .child(
                                        div()
                                            .p_4()
                                            .w_full()
                                            .flex()
                                            .justify_center()
                                            .child(
                                                Label::new("EXIT_TO_ONBOARDING")
                                                    .size(LabelSize::XSmall)
                                            )
                                    )
                            )
                    )
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
        "/void".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("New Welcome Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(crate::item::ItemEvent)) {
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
            &persistence::WelcomePagesDb::global(cx),
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
        if persistence::WelcomePagesDb::global(cx)
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
        let db = persistence::WelcomePagesDb::global(cx);
        Some(cx.background_spawn(
            async move { db.save_welcome_page(item_id, workspace_id, true).await },
        ))
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

    db::static_connection!(WelcomePagesDb, [WorkspaceDb]);

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

fn project_name(paths: &PathList) -> String {
    let joined = paths
        .paths()
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        "Untitled".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_name_empty() {
        let paths = PathList::new::<&str>(&[]);
        assert_eq!(project_name(&paths), "Untitled");
    }

    #[test]
    fn test_project_name_single() {
        let paths = PathList::new(&["/home/user/my-project"]);
        assert_eq!(project_name(&paths), "my-project");
    }

    #[test]
    fn test_project_name_multiple() {
        // PathList sorts lexicographically, so filenames appear in alpha order
        let paths = PathList::new(&["/home/user/zed", "/home/user/api"]);
        assert_eq!(project_name(&paths), "api, zed");
    }

    #[test]
    fn test_project_name_root_path_filtered() {
        // A bare root "/" has no file_name(), falls back to "Untitled"
        let paths = PathList::new(&["/"]);
        assert_eq!(project_name(&paths), "Untitled");
    }
}
