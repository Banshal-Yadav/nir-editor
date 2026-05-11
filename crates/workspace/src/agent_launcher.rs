use crate::{
    SplitDirection, StatusItemView, Workspace,
    agent_launcher_page::AgentLauncherPage,
};
use gpui::{Context, Entity, Window};
use ui::{prelude::*, Tooltip};

pub struct AgentLauncherButton {
    workspace: Entity<Workspace>,
}

impl AgentLauncherButton {
    pub fn new(workspace: Entity<Workspace>) -> Self {
        Self { workspace }
    }
}

impl Render for AgentLauncherButton {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();
        Button::new("agent-launcher", "Terminal Agent")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Subtle)
            .color(Color::Muted)
            .start_icon(Icon::new(IconName::Terminal).color(Color::Muted))
            .tooltip(Tooltip::text("Launch Terminal AI Agents"))
            .on_click(move |_clicked, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    //  1. If a launcher tab already exists anywhere, just focus it 
                    // Collect eagerly so the immutable borrow on workspace/cx from the
                    // iterator is dropped before we call activate_item (mutable borrow).
                    let existing = workspace
                        .items_of_type::<AgentLauncherPage>(cx)
                        .next();
                    if let Some(existing) = existing {
                        workspace.activate_item(&existing, true, true, window, cx);
                        return;
                    }

                    // ── 2. No existing tab — decide where to open it ──
                    let item = cx.new(|cx| {
                        AgentLauncherPage::new(workspace.weak_handle(), cx)
                    });

                    // Count editor/file items in the active pane (ignore non-file items)
                    let active_pane_has_files = workspace
                        .active_pane()
                        .read(cx)
                        .items()
                        .any(|i| i.project_path(cx).is_some());

                    if active_pane_has_files {
                        // Files are open → open launcher in a split to the right
                        workspace.split_item(
                            SplitDirection::Right,
                            Box::new(item),
                            window,
                            cx,
                        );
                    } else {
                        // Empty workspace or no files open → open in active pane
                        workspace.add_item_to_active_pane(
                            Box::new(item),
                            None,
                            true,
                            window,
                            cx,
                        );
                    }
                });
            })
    }
}

impl StatusItemView for AgentLauncherButton {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn crate::ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}