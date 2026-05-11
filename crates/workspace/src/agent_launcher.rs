use crate::{SplitDirection, StatusItemView, Workspace};
use gpui::{Context, Entity, Window};
use task::{
    HideStrategy, RevealStrategy, RevealTarget, SaveStrategy, Shell, SpawnInTerminal, TaskId,
};
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
                    if workspace.items(cx).next().is_some() {
                        let active_pane = workspace.active_pane().clone();
                        workspace.split_pane(active_pane, SplitDirection::Right, window, cx);
                    }

                    let action = SpawnInTerminal {
                        id: TaskId("terminal-agent".into()),
                        full_label: "Terminal Agent".into(),
                        label: "Terminal Agent".into(),
                        command: Some("npx @google/gemini-cli".into()),
                        args: Vec::new(),
                        command_label: "npx @google/gemini-cli".into(),
                        cwd: None,
                        env: Default::default(),
                        use_new_terminal: true,
                        allow_concurrent_runs: true,
                        reveal: RevealStrategy::Always,
                        reveal_target: RevealTarget::Center,
                        hide: HideStrategy::Never,
                        shell: Shell::System,
                        show_summary: false,
                        show_command: false,
                        show_rerun: false,
                        save: SaveStrategy::default(),
                    };
                    workspace.spawn_in_terminal(action, window, cx).detach();
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