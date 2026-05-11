use crate::{StatusItemView, Workspace};
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
            .tooltip(Tooltip::text("Launch Terminal Agent"))
            .on_click(move |_clicked, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    let item = cx.new(|cx| {
                        crate::agent_launcher_page::AgentLauncherPage::new(workspace.weak_handle(), cx)
                    });
                    if workspace.items(cx).next().is_some() {
                        workspace.split_item(crate::SplitDirection::Right, Box::new(item), window, cx);
                    } else {
                        workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);
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