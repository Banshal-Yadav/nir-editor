use crate::{ItemHandle, NewCenterTerminal, StatusItemView, Workspace};
use futures_lite::future::yield_now;
use gpui::{Action, Context, Entity, Task, Window};
use std::process::ExitStatus;
use task::{RevealStrategy, SpawnInTerminal, TaskId};
use ui::prelude::*;
use util::shell::Shell;

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
            .on_click(move |_clicked, window, cx| {
                let spawn = SpawnInTerminal {
                    id: TaskId("void-agent-launcher".to_string()),
                    full_label: "Terminal Agent".to_string(),
                    label: "Terminal Agent".to_string(),
                    command: Some("npx".to_string()),
                    args: vec!["@google/gemini-cli".to_string()],
                    command_label: "Gemini".to_string(),
                    cwd: None,
                    env: Default::default(),
                    use_new_terminal: true,
                    allow_concurrent_runs: true,
                    reveal: RevealStrategy::Always,
                    reveal_target: Default::default(),
                    hide: Default::default(),
                    shell: Shell::System,
                    show_summary: true,
                    show_command: true,
                    show_rerun: true,
                    save: Default::default(),
                };
                window.dispatch_action(NewCenterTerminal::default().boxed_clone(), cx);

                let workspace = workspace.clone();
                window
                    .spawn(cx, async move |cx| {
                        for _ in 0..50 {
                            let ready = workspace.update(cx, |workspace, _| {
                                workspace.terminal_provider.is_some()
                            });
                            if ready {
                                break;
                            }
                            yield_now().await;
                        }

                        let task: Option<Task<Option<anyhow::Result<ExitStatus>>>> = workspace
                            .update_in(cx, |workspace, window, cx| {
                                if workspace.terminal_provider.is_none() {
                                    return None;
                                }
                                Some(workspace.spawn_in_terminal(spawn, window, cx))
                            })
                            .ok()
                            .flatten();

                        if let Some(task) = task {
                            cx.background_spawn(async move {
                                let _ = task.await;
                            });
                        }
                    })
                    .detach();
            })
    }
}

impl StatusItemView for AgentLauncherButton {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        
    }
}