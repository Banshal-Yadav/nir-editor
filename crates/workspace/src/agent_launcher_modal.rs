use gpui::{
    App, AsyncApp, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, Render, WeakEntity, Window,
};
use ui::prelude::*;
use crate::{Workspace, SplitDirection};
use task::{
    HideStrategy, RevealStrategy, RevealTarget, SaveStrategy, Shell, SpawnInTerminal, TaskId,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentStatus {
    Checking,
    Installed,
    NotInstalled,
}

struct Agent {
    name: &'static str,
    description: &'static str,
    binary: &'static str,
    command: &'static str,
    install_command: &'static str,
    docs_url: &'static str,
    status: AgentStatus,
}

pub struct AgentLauncherModal {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    agents: Vec<Agent>,
}

impl AgentLauncherModal {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let mut agents = vec![
            Agent {
                name: "Gemini CLI",
                description: "Google's powerful multimodal AI agent.",
                binary: "npx",
                command: "npx @google/gemini-cli",
                install_command: "npx handles it",
                docs_url: "https://github.com/google/gemini-cli",
                status: AgentStatus::Installed,
            },
            Agent {
                name: "Claude Code",
                description: "Anthropic's high-performance coding assistant.",
                binary: "claude",
                command: "claude",
                install_command: "npm install -g @anthropic-ai/claude-code",
                docs_url: "https://claude.ai/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Codex",
                description: "OpenAI's foundational model for code.",
                binary: "codex",
                command: "codex",
                install_command: "npm install -g @openai/codex",
                docs_url: "https://openai.com/blog/openai-codex",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Aider",
                description: "AI pair programming in your terminal.",
                binary: "aider",
                command: "aider",
                install_command: "pip install aider-chat",
                docs_url: "https://aider.chat/",
                status: AgentStatus::Checking,
            },
        ];

        let focus_handle = cx.focus_handle();
        
        // Background check for binaries
        for (i, agent) in agents.iter_mut().enumerate() {
            if agent.status == AgentStatus::Checking {
                let binary = agent.binary;
                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        let installed = check_binary(binary).await;
                        let _ = this.update(&mut cx, |this, cx: &mut Context<Self>| {
                            this.agents[i].status = if installed {
                                AgentStatus::Installed
                            } else {
                                AgentStatus::NotInstalled
                            };
                            cx.notify();
                        });
                    }
                })
                .detach();
            }
        }

        Self {
            workspace,
            focus_handle,
            agents,
        }
    }

    fn launch_agent(&mut self, agent_name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let (command, name) = {
                let agent = self.agents.iter().find(|a| a.name == agent_name).unwrap();
                (agent.command.to_string(), agent.name.to_string())
            };
            
            workspace.update(cx, |workspace, cx| {
                if workspace.items(cx).next().is_some() {
                    let active_pane = workspace.active_pane().clone();
                    workspace.split_pane(active_pane, SplitDirection::Right, window, cx);
                }

                let action = SpawnInTerminal {
                    id: TaskId(format!("terminal-agent-{}", name.to_lowercase().replace(' ', "-"))),
                    full_label: format!("Launch {}", name),
                    label: name.clone(),
                    command: Some(command.clone()),
                    args: Vec::new(),
                    command_label: command,
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
            cx.emit(DismissEvent);
        }
    }
}

async fn check_binary(binary: &str) -> bool {
    #[cfg(windows)]
    let cmd = "where";
    #[cfg(not(windows))]
    let cmd = "which";

    let mut cmd = util::command::Command::new(cmd);
    cmd.arg(binary);
    matches!(
        cmd.output().await,
        Ok(o) if o.status.success()
    )
}

impl EventEmitter<DismissEvent> for AgentLauncherModal {}

impl Focusable for AgentLauncherModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentLauncherModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(rems(40.))
            .bg(cx.theme().colors().editor_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_lg()
            .shadow_lg()
            .child(
                v_flex()
                    .p_6()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .text_color(cx.theme().colors().text_accent)
                                    .child("[/]"),
                            )
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .child("nir agent Launcher"),
                            ),
                    )
                    .child(
                        Label::new("Run any AI agent in your IDE")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                v_flex()
                    .p_2()
                    .gap_2()
                    .children(self.agents.iter().map(|agent| {
                        let agent_name = agent.name;
                        let is_installed = agent.status == AgentStatus::Installed;
                        let is_checking = agent.status == AgentStatus::Checking;
                        
                        v_flex()
                            .p_4()
                            .border_1()
                            .border_color(cx.theme().colors().border)
                            .rounded_md()
                            .bg(cx.theme().colors().editor_background.opacity(0.5))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        h_flex()
                                            .gap_3()
                                            .items_center()
                                            .child(
                                                Icon::new(IconName::Terminal)
                                                    .color(Color::Muted),
                                            )
                                            .child(
                                                v_flex()
                                                    .child(Label::new(agent_name).weight(gpui::FontWeight::BOLD))
                                                    .child(Label::new(agent.description).size(LabelSize::Small).color(Color::Muted))
                                            )
                                    )
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .when(is_checking, |this| this.child(Label::new("Checking...").size(LabelSize::XSmall).color(Color::Muted)))
                                            .when(!is_checking && is_installed, |this| {
                                                this.child(Icon::new(IconName::Check).color(Color::Success))
                                                    .child(
                                                        Button::new(format!("launch-{}", agent_name), "Launch")
                                                            .style(ButtonStyle::Filled)
                                                            .on_click(cx.listener({
                                                                let agent_name = agent.name;
                                                                move |this, _, window, cx| {
                                                                    this.launch_agent(agent_name, window, cx);
                                                                }
                                                            }))
                                                    )
                                            })
                                            .when(!is_checking && !is_installed, |this| {
                                                this.child(Icon::new(IconName::Warning).color(Color::Warning))
                                            })
                                    )
                            )
                            .when(!is_installed && !is_checking, |this| {
                                this.child(
                                    v_flex()
                                        .mt_4()
                                        .gap_2()
                                        .child(
                                            div()
                                                .bg(cx.theme().colors().editor_background.opacity(0.8))
                                                .p_3()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(cx.theme().colors().border)
                                                .child(Label::new(agent.install_command).size(LabelSize::XSmall).color(Color::Accent))
                                        )
                                        .child(
                                            h_flex()
                                                .justify_end()
                                                .child(
                                                    Button::new(format!("docs-{}", agent_name), "View Docs")
                                                        .style(ButtonStyle::Subtle)
                                                        .on_click({
                                                            let url = agent.docs_url;
                                                            move |_, _, cx| cx.open_url(url)
                                                        })
                                                )
                                        )
                                )
                            })
                    }))
            )
            .child(
                h_flex()
                    .p_4()
                    .justify_end()
                    .child(
                        Button::new("close", "Close")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
                    )
            )
    }
}

impl crate::ModalView for AgentLauncherModal {
    fn fade_out_background(&self) -> bool {
        true
    }
}
