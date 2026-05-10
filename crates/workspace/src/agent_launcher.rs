use crate::{ItemHandle, NewCenterTerminal, StatusItemView, Workspace};
use futures_lite::future::yield_now;
use gpui::{Action, Context, Entity, Window};
use ui::prelude::*;

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
                window.dispatch_action(NewCenterTerminal::default().boxed_clone(), cx);

                let workspace = workspace.clone();
                window
                    .spawn(cx, async move |cx| {
                        for _ in 0..100 {
                            let terminal = workspace.update(cx, |workspace, cx| {
                                workspace
                                    .active_item(cx)
                                    .and_then(|item| item.act_as_terminal(cx))
                            });

                            if let Some(terminal) = terminal {
                                terminal.update(cx, |terminal, _| {
                                    terminal.input(b"npx @google/gemini-cli\x0d".to_vec());
                                });
                                return;
                            }
                            yield_now().await;
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