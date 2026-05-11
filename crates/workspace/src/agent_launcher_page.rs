use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, WeakEntity, Window,
    SharedString,
};
use ui::prelude::*;
use ui::Tooltip;
use crate::{Workspace, item::{Item, ItemEvent}};
use task::{
    HideStrategy, RevealStrategy, RevealTarget, SaveStrategy, Shell, SpawnInTerminal, TaskId,
};
use std::collections::HashSet;

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

pub struct AgentLauncherPage {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    agents: Vec<Agent>,
    expanded_indices: HashSet<usize>,
}

impl AgentLauncherPage {
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
                binary: "npx",
                command: "npx -y @openai/codex",
                install_command: "npx handles it",
                docs_url: "https://openai.com/blog/openai-codex",
                status: AgentStatus::Installed,
            },
            Agent {
                name: "Hermes",
                description: "Nous Research's self-improving autonomous agent.",
                binary: "hermes",
                command: "hermes",
                install_command: if cfg!(windows) { "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash" } else { "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash" },
                docs_url: "https://github.com/NousResearch/hermes-agent",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "ZAI (GLM)",
                description: "General Language Model (GLM) optimized for terminal coding.",
                binary: "zai",
                command: "zai",
                install_command: "npm install -g @guizmo-ai/zai-cli",
                docs_url: "https://z.ai/",
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
            Agent {
                name: "Open Interpreter",
                description: "Open-source, local implementation of OpenAI's Code Interpreter.",
                binary: "interpreter",
                command: "interpreter",
                install_command: "pip install open-interpreter",
                docs_url: "https://openinterpreter.com/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Plandex",
                description: "AI coding agent for complex tasks.",
                binary: "plandex",
                command: "plandex",
                install_command: if cfg!(windows) { "Requires WSL. Run: curl -sL https://plandex.ai/install.sh | bash" } else { "curl -sL https://plandex.ai/install.sh | bash" },
                docs_url: "https://plandex.ai/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Mentat",
                description: "AI tool that coordinates edits across multiple files.",
                binary: "mentat",
                command: "mentat",
                install_command: "pip install mentat",
                docs_url: "https://mentat.ai/",
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
            expanded_indices: HashSet::new(),
        }
    }

    fn launch_agent(&mut self, agent_name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let (command, name) = {
                let agent = self.agents.iter().find(|a| a.name == agent_name).unwrap();
                (agent.command.to_string(), agent.name.to_string())
            };
            
            workspace.update(cx, |workspace, cx| {
                let action = SpawnInTerminal {
                    id: TaskId(format!("terminal-agent-{}", name.to_lowercase().replace(' ', "-"))),
                    full_label: format!("Launch {}", name),
                    label: name.clone(),
                    command: Some(command.clone()),
                    args: Vec::new(),
                    command_label: command,
                    cwd: None,
                    env: Default::default(),
                    use_new_terminal: false,
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
            cx.emit(ItemEvent::CloseItem);
        }
    }

    fn toggle_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.expanded_indices.contains(&index) {
            self.expanded_indices.remove(&index);
        } else {
            self.expanded_indices.insert(index);
        }
        cx.notify();
    }
}

async fn check_binary(binary: &str) -> bool {
    #[cfg(windows)]
    let cmd = "where";
    #[cfg(not(windows))]
    let cmd = "which";

    match std::process::Command::new(cmd).arg(binary).output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

impl EventEmitter<ItemEvent> for AgentLauncherPage {}

impl Focusable for AgentLauncherPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentLauncherPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().colors().border;
        let accent_color = cx.theme().colors().text_accent;

        h_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(rems(48.))
                    .p_12()
                    .gap_8()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_size(px(32.))
                                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                            .text_color(accent_color)
                                            .child("[/]"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(32.))
                                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                            .child("terminal agent launcher"),
                                    ),
                            )
                            .child(
                                Label::new("Select an AI agent to run in your terminal environment")
                                    .size(LabelSize::Default)
                                    .color(Color::Muted),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h_px()
                            .bg(border_color),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .gap_4()
                            .children(self.agents.iter().enumerate().map(|(i, agent)| {
                                let agent_name = agent.name;
                                let is_installed = agent.status == AgentStatus::Installed;
                                let is_checking = agent.status == AgentStatus::Checking;
                                let is_expanded = self.expanded_indices.contains(&i);
                                
                                v_flex()
                                    .w_full()
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_md()
                                    .child(
                                        div()
                                            .id(("agent-row", i))
                                            .flex()
                                            .w_full()
                                            .justify_between()
                                            .items_center()
                                            .p_4()
                                            .cursor_pointer()
                                            .hover(|style| style.bg(cx.theme().colors().element_hover))
                                            .on_click(cx.listener(move |this, _, _, cx| this.toggle_expanded(i, cx)))
                                            .child(
                                                h_flex()
                                                    .gap_4()
                                                    .items_center()
                                                    .child(
                                                        Icon::new(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                                                            .size(IconSize::XSmall)
                                                            .color(Color::Muted)
                                                    )
                                                    .child(
                                                        Label::new(agent_name)
                                                            .weight(gpui::FontWeight::BOLD)
                                                            .size(LabelSize::Default)
                                                    )
                                                    .when(is_installed, |this| {
                                                        this.child(Icon::new(IconName::Check).color(Color::Success).size(IconSize::XSmall))
                                                    })
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_4()
                                                    .items_center()
                                                    .when(is_checking, |this| {
                                                        this.child(Label::new("probing...").size(LabelSize::XSmall).color(Color::Muted))
                                                    })
                                                    .when(!is_checking && is_installed, |this| {
                                                        this.child(
                                                            Button::new(format!("launch-{}", agent_name), "LAUNCH")
                                                                .style(ButtonStyle::Filled)
                                                                .on_click(cx.listener({
                                                                    let agent_name = agent.name;
                                                                    move |this, _, window, cx| {
                                                                        this.launch_agent(agent_name, window, cx);
                                                                    }
                                                                }))
                                                        )
                                                    })
                                            )
                                    )
                                    .when(is_expanded, |this| {
                                        this.child(
                                            v_flex()
                                                .p_4()
                                                .bg(cx.theme().colors().editor_background.opacity(0.5))
                                                .border_t_1()
                                                .border_color(border_color)
                                                .gap_4()
                                                .child(
                                                    Label::new(agent.description)
                                                        .size(LabelSize::Small)
                                                        .color(Color::Default)
                                                )
                                                .child(
                                                    v_flex()
                                                        .gap_2()
                                                        .child(Label::new("Installation Guide").size(LabelSize::XSmall).weight(gpui::FontWeight::BOLD))
                                                        .child(
                                                            h_flex()
                                                                .items_center()
                                                                .bg(cx.theme().colors().editor_background.opacity(0.8))
                                                                .p_3()
                                                                .rounded_md()
                                                                .border_1()
                                                                .border_color(border_color)
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .flex_1()
                                                                        .child(Label::new(agent.install_command).size(LabelSize::XSmall).color(Color::Accent))
                                                                )
                                                                .child(
                                                                    IconButton::new("copy-command", IconName::Copy)
                                                                        .icon_size(IconSize::XSmall)
                                                                        .on_click({
                                                                            let cmd = agent.install_command.to_string();
                                                                            move |_, _, cx| {
                                                                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd.clone()));
                                                                            }
                                                                        })
                                                                        .tooltip(move |window, cx| Tooltip::text("Copy to clipboard")(window, cx))
                                                                )
                                                        )
                                                )
                                                .child(
                                                    h_flex()
                                                        .justify_end()
                                                        .child(
                                                            Button::new(format!("docs-{}", agent_name), "Documentation")
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
                        v_flex()
                            .mt_auto()
                            .child(
                                Label::new("VOID v0.1.0-agentic")
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                            )
                    )
            )
    }
}

impl Item for AgentLauncherPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Agent Launcher".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Agent Launcher Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<crate::WorkspaceId>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Option<Entity<Self>>> {
        gpui::Task::ready(Some(cx.new(|cx| AgentLauncherPage::new(self.workspace.clone(), cx))))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}
