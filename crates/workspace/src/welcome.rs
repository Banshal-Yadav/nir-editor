use crate::{
    NewFile, Open,
    Workspace,
    item::{Item, ItemEvent},
};
use gpui::{
    px, relative, App, AppContext, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    Entity, Render, WeakEntity,
    actions, Task, Window, Animation, AnimationExt, pulsating_between,
    linear_gradient, linear_color_stop, white,
};
use menu::{SelectNext, SelectPrevious};

use ui::{Icon, IconName, Label, prelude::*};
use zed_actions::{
    OpenKeymap, OpenSettings, assistant::ToggleFocus,
};
use git::Clone as GitClone;
use zed_actions::command_palette::Toggle as ToggleCommandPalette;
use zed_actions::OpenOnboarding;


actions!(
    zed,
    [
        /// Show the /void welcome screen
        ShowWelcome
    ]
);

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
            .gap_2()
            .child(
                div()
                    .font_family("Intel One Mono")
                    .text_size(px(28.))
                    .line_height(relative(1.))
                    .font_weight(FontWeight::BLACK)
                    .text_color(cx.theme().colors().text_accent)
                    .child("[/]"),
            )
            .child(
                div()
                    .font_family("Intel One Mono")
                    .text_size(px(28.))
                    .line_height(relative(1.))
                    .font_weight(FontWeight::BLACK)
                    .child("nir"),
            )
            .child(div().w(px(4.)))
            .child(
                div()
                    .text_size(px(28.))
                    .line_height(relative(1.))
                    .text_color(cx.theme().colors().text_accent)
                    .child("▊")
                    .with_animation("void-cursor", cursor_animation, |el, delta| {
                        el.opacity(delta)
                    }),
            )
    }
}

pub struct WelcomePage {
    focus_handle: FocusHandle,

}

impl WelcomePage {
    pub fn new(
        _workspace: WeakEntity<Workspace>,
        _fallback_to_recent_projects: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, _window, |_, _, cx| cx.notify())
            .detach();

        WelcomePage {
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


}

impl Render for WelcomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle.clone();
        let hover_bg = cx.theme().colors().element_hover;
        let border_color = cx.theme().colors().border;

        h_flex()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .justify_center()
            .items_center()
            .child(
                v_flex()
                    .w(rems(36.))
                    .border_1()
                    .border_color(border_color)
                    .rounded_lg()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .w_full()
                            .h(rems(5.))
                            .px_6()
                            .border_b_1()
                            .border_color(border_color)
                            .items_center()
                            .justify_between()
                            .child(VoidLogo::new(cx))
                            .child(
                                Label::new("THINK. BUILD. SHIP.")
                                    .weight(FontWeight::BOLD)
                                    .size(LabelSize::Default)
                                    .color(Color::Accent),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px_6()
                            .py_3()
                            .border_b_1()
                            .border_color(border_color)
                            .child(
                                Label::new("OR GET STARTED")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    )
                    .child(
                        div()
                            .id("action-new-file")
                            .flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .px_6()
                            .py_4()
                            .border_b_1()
                            .border_color(border_color)
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover_bg))
                            .on_click({
                                let focus = focus.clone();
                                move |_, window, cx| focus.dispatch_action(&NewFile, window, cx)
                            })
                            .child(
                                h_flex()
                                    .gap_4()
                                    .items_center()
                                    .child(Icon::new(IconName::Plus).color(Color::Muted))
                                    .child(
                                        v_flex()
                                            .child(Label::new("NEW FILE").weight(FontWeight::BOLD).size(LabelSize::Default))
                                            .child(Label::new("initialize empty buffer").size(LabelSize::Small).color(Color::Muted)),
                                    )
                            )
                            .child(
                                div().border_1().border_color(border_color).px_2().py_1().rounded_md()
                                    .child(Label::new("Ctrl-N").size(LabelSize::Small).color(Color::Muted)),
                            ),
                    )
                    .child(
                        div()
                            .id("action-open-project")
                            .flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .px_6()
                            .py_4()
                            .border_b_1()
                            .border_color(border_color)
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover_bg))
                            .on_click({
                                let focus = focus.clone();
                                move |_, window, cx| focus.dispatch_action(&Open::DEFAULT, window, cx)
                            })
                            .child(
                                h_flex()
                                    .gap_4()
                                    .items_center()
                                    .child(Icon::new(IconName::Folder).color(Color::Muted))
                                    .child(
                                        v_flex()
                                            .child(Label::new("OPEN PROJECT").weight(FontWeight::BOLD).size(LabelSize::Default))
                                            .child(Label::new("load workspace from disk").size(LabelSize::Small).color(Color::Muted)),
                                    )
                            )
                            .child(
                                div().border_1().border_color(border_color).px_2().py_1().rounded_md()
                                    .child(Label::new("Ctrl-K Ctrl-O").size(LabelSize::Small).color(Color::Muted)),
                            ),
                    )
                    .child(
                        div()
                            .id("action-clone-repo")
                            .flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .px_6()
                            .py_4()
                            .border_b_1()
                            .border_color(border_color)
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover_bg))
                            .on_click({
                                let focus = focus.clone();
                                move |_, window, cx| focus.dispatch_action(&GitClone, window, cx)
                            })
                            .child(
                                h_flex()
                                    .gap_4()
                                    .items_center()
                                    .child(Icon::new(IconName::CloudDownload).color(Color::Muted))
                                    .child(
                                        v_flex()
                                            .child(Label::new("CLONE REPOSITORY").weight(FontWeight::BOLD).size(LabelSize::Default))
                                            .child(Label::new("git clone into new workspace").size(LabelSize::Small).color(Color::Muted)),
                                    )
                            ),
                    )
                    .child(
                        div()
                            .id("action-settings")
                            .flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .px_6()
                            .py_4()
                            .border_b_1()
                            .border_color(border_color)
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover_bg))
                            .on_click({
                                let focus = focus.clone();
                                move |_, window, cx| focus.dispatch_action(&OpenSettings, window, cx)
                            })
                            .child(
                                h_flex()
                                    .gap_4()
                                    .items_center()
                                    .child(Icon::new(IconName::Settings).color(Color::Muted))
                                    .child(
                                        v_flex()
                                            .child(Label::new("SETTINGS").weight(FontWeight::BOLD).size(LabelSize::Default))
                                            .child(Label::new("configure system preferences").size(LabelSize::Small).color(Color::Muted)),
                                    )
                            )
                            .child(
                                div().border_1().border_color(border_color).px_2().py_1().rounded_md()
                                    .child(Label::new("Ctrl-,").size(LabelSize::Small).color(Color::Muted)),
                            ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .p_6()
                            .child(
                                v_flex()
                                    .w_full()
                                    .bg(linear_gradient(
                                        180.0,
                                        linear_color_stop(cx.theme().colors().editor_background.blend(white().alpha(0.03)), 0.0),
                                        linear_color_stop(cx.theme().colors().editor_background, 1.0),
                                    ))
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_lg()
                                    .p_4()
                                    .child(
                                        Label::new("Collaborate with Agents")
                                            .weight(FontWeight::BOLD)
                                            .size(LabelSize::Default),
                                    )
                                    .child(
                                        Label::new("Run multiple threads at once, mix and match any ACP-compatible agent, and keep work conflict-free with worktrees.")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .my_3(),
                                    )
                                    .child(
                                        div()
                                            .id("open-agent-panel-btn")
                                            .w_full()
                                            .border_1()
                                            .border_color(border_color)
                                            .rounded_md()
                                            .py_2()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(hover_bg))
                                            .on_click({
                                                let focus = focus.clone();
                                                move |_, window, cx| {
                                                    focus.dispatch_action(&ToggleFocus, window, cx);
                                                }
                                            })
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .child(Label::new("Open Agent Panel").size(LabelSize::Small))
                                                    .child(Label::new("Ctrl-Shift-/").size(LabelSize::Small).color(Color::Muted)),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .h(rems(2.8))
                            .border_t_1()
                            .border_color(border_color)
                            .child(
                                div().id("btn-keymaps").flex_1().h_full().flex().items_center().justify_center().border_r_1().border_color(border_color).cursor_pointer()
                                    .hover(move |style| style.bg(hover_bg))
                                    .on_click({
                                        let focus = focus.clone();
                                        move |_, window, cx| focus.dispatch_action(&OpenKeymap, window, cx)
                                    })
                                    .child(Label::new("KEYMAPS").size(LabelSize::Small)),
                            )
                            .child(
                                div().id("btn-command-palette").flex_1().h_full().flex().items_center().justify_center().border_r_1().border_color(border_color).cursor_pointer()
                                    .hover(move |style| style.bg(hover_bg))
                                    .on_click({
                                        let focus = focus.clone();
                                        move |_, window, cx| focus.dispatch_action(&ToggleCommandPalette, window, cx)
                                    })
                                    .child(Label::new("COMMAND PALETTE").size(LabelSize::Small)),
                            )
                            .child(
                                div()
                                    .id("btn-onboarding")
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .bg(cx.theme().colors().text_accent.alpha(0.12))
                                    .hover(|style| {
                                        style.bg(cx.theme().colors().text_accent.alpha(0.2))
                                    })
                                    .on_click({
                                        let focus = focus.clone();
                                        move |_, window, cx| focus.dispatch_action(&OpenOnboarding, window, cx)
                                    })
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .items_center()
                                            .child(
                                                Label::new("ONBOARDING")
                                                    .size(LabelSize::Small)
                                                    .color(Color::Accent),
                                            )
                                            .child(
                                                Icon::new(IconName::ChevronRight)
                                                    .size(IconSize::XSmall)
                                                    .color(Color::Accent),
                                            ),
                                    ),
                            ),
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
        "/nir".into()
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