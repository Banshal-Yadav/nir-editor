use gpui::{AppContext, Context, Entity, Window};
use task::{SpawnInTerminal, TaskId};
use ui::prelude::*;
use util::shell::Shell;
use workspace::{StatusItemView, Workspace};

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
        Button::new("agent-launcher", "AI Agent")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Subtle)
            .on_click(|_clicked, window, cx| {
                let workspace = cx
                    .global::<AppState>()
                    .workspace_store
                    .read(cx)
                    .active()
                    .cloned()
                    .unwrap_or_else(|| cx.entity());

                let spawn = SpawnInTerminal {
                    id: TaskId("nir-agent-launcher".to_string()),
                    label: "AI Agent".to_string(),
                    command: Some("opencode".to_string()),
                    args: vec![],
                    cwd: None,
                    env: Default::default(),
                    use_new_terminal: true,
                    allow_concurrent_runs: true,
                    reveal: Default::default(),
                    reveal_target: Default::default(),
                    hide: Default::default(),
                    shell: Shell::default(),
                    show_summary: true,
                    show_command: true,
                    show_rerun: true,
                    save: Default::default(),
                };

                let workspace_clone = workspace.clone();
                workspace.update(cx, |workspace, window, cx| {
                    workspace.spawn_in_terminal(spawn, window, cx);
                });
            })
    }
}

impl StatusItemView for AgentLauncherButton {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn workspace::ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Nothing to do - this button doesn't depend on the active item
    }
}