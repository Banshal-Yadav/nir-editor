use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, WeakEntity, Window,
    SharedString, StatefulInteractiveElement,
};
use ui::prelude::*;
use ui::Tooltip;
use crate::{Workspace, item::{Item, ItemEvent}};
use task::{
    HideStrategy, RevealStrategy, RevealTarget, SaveStrategy, Shell, SpawnInTerminal, TaskId,
};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentStatus {
    Checking,
    Installed,
    NotInstalled,
}

struct Agent {
    name: &'static str,
    /// Short single-line description shown in expanded panel
    description: &'static str,
    /// Requirements shown as a badge/hint (runtime, platform caveats)
    requires: &'static str,
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
    copied_indices: HashSet<usize>,
    is_refreshing: bool,
    pending_probes: usize,
}

impl AgentLauncherPage {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let agents = vec![
            Agent {
                name: "Gemini CLI",
                description: "Google's multimodal AI agent for terminal-based chat, code generation, and project analysis. Free tier available with Google account.",
                requires: "Node.js 18+",
                binary: "npx",
                command: "npx @google/gemini-cli",
                install_command: "npx @google/gemini-cli  # no install needed",
                docs_url: "https://github.com/google/gemini-cli",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Claude Code",
                description: "Anthropic's agentic coding tool. Reads files, runs commands, and edits code across your entire project. Requires Anthropic API key or Pro/Team subscription.",
                requires: "Node.js 18+ • Anthropic account",
                binary: "claude",
                command: "claude",
                install_command: "npm install -g @anthropic-ai/claude-code",
                docs_url: "https://docs.anthropic.com/en/docs/claude-code",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "OpenAI Codex",
                description: "OpenAI's lightweight terminal coding agent. Runs tasks, edits files, and handles multi-step reasoning. Requires OPENAI_API_KEY environment variable.",
                requires: "Node.js 18+ • OPENAI_API_KEY",
                binary: "codex",
                command: "npx -y @openai/codex",
                install_command: "npx -y @openai/codex  # no install needed",
                docs_url: "https://github.com/openai/codex",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "OpenCode",
                description: "Open-source TUI agent that analyzes and evolves entire codebases. Supports OpenAI, Anthropic, Gemini, Ollama and more. Run /init to analyze your project.",
                requires: "Node.js 18+ • Modern terminal (true color)",
                binary: "opencode",
                command: "opencode",
                install_command: if cfg!(windows) { "npm install -g opencode-ai" } else { "curl -fsSL https://opencode.ai/install | bash" },
                docs_url: "https://opencode.ai/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Aider",
                description: "AI pair programmer with deep Git integration. Supports 100+ LLMs. Works best with Claude Sonnet or GPT-4o. Requires an API key for your chosen provider.",
                requires: "Python 3.8–3.13 • Git • API key",
                binary: "aider",
                command: "aider",
                install_command: "python -m pip install aider-install && aider-install",
                docs_url: "https://aider.chat/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Open Interpreter",
                description: "Executes code locally to complete tasks: browse the web, create files, control apps. Supports local models via Ollama. Review code before approving execution.",
                requires: "Python 3.10–3.11 • API key or Ollama",
                binary: "interpreter",
                command: "interpreter",
                install_command: "pip install open-interpreter",
                docs_url: "https://openinterpreter.com/",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Hermes Agent",
                description: "NousResearch's self-improving autonomous agent. Installs its own environment (Python 3.11, Node.js). Windows users should use WSL2 for best results.",
                requires: "curl • bash • (WSL2 on Windows)",
                binary: "hermes",
                command: "hermes",
                install_command: if cfg!(windows) {
                    "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex"
                } else {
                    "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash"
                },
                docs_url: "https://github.com/NousResearch/hermes-agent",
                status: AgentStatus::Checking,
            },
            Agent {
                name: "Plandex",
                description: "Multi-file AI coding agent with a client-server architecture. Excels at large, complex engineering tasks spanning many files. Windows requires WSL.",
                requires: "Linux / macOS / WSL2 • API key",
                binary: "plandex",
                command: "plandex",
                install_command: "curl -sL https://plandex.ai/install.sh | bash",
                docs_url: "https://plandex.ai/",
                status: AgentStatus::Checking,
            },
        ];

        let focus_handle = cx.focus_handle();
        let mut this = Self {
            workspace,
            focus_handle,
            agents,
            expanded_indices: HashSet::new(),
            copied_indices: HashSet::new(),
            is_refreshing: false,
            pending_probes: 0,
        };
        
        this.check_all_binaries(cx);
        
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor().timer(Duration::from_secs(15)).await;
                    let success = this.update(&mut cx, |this, cx| {
                        this.check_all_binaries(cx);
                    }).is_ok();
                    if !success { break; }
                }
            }
        }).detach();

        this
    }

    fn check_all_binaries(&mut self, cx: &mut Context<Self>) {
        self.is_refreshing = true;
        self.pending_probes = self.agents.len();
        cx.notify();

        for (i, agent) in self.agents.iter().enumerate() {
            let binary = agent.binary;
            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let installed = check_binary(binary).await;
                    let _ = this.update(&mut cx, |this, cx: &mut Context<Self>| {
                        let new_status = if installed { AgentStatus::Installed } else { AgentStatus::NotInstalled };
                        this.agents[i].status = new_status;
                        // Decrement shared counter — when zero, clear the spinner
                        this.pending_probes = this.pending_probes.saturating_sub(1);
                        if this.pending_probes == 0 {
                            this.is_refreshing = false;
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
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
            // we intentionally do NOT close the launcher tab here.
            // Keeping it alive means clicking the status bar button again will
            // focus this existing tab instead of failing to reopen.
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
    
    fn copy_install_command(&mut self, index: usize, cmd: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd));
        self.copied_indices.insert(index);
        cx.notify();
        
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let _ = this.update(&mut cx, |this, cx| {
                    this.copied_indices.remove(&index);
                    cx.notify();
                });
            }
        }).detach();
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
        let surface_color = cx.theme().colors().elevated_surface_background;
        let success_color = cx.theme().status().success;
        let warning_color = cx.theme().status().warning;
        let is_refreshing = self.is_refreshing;

        div()
            .id("agent-launcher-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().colors().editor_background)
            // ─── Header ───────────────────────────────────────────────
            .child(
                h_flex()
                    .w_full()
                    .px_5()
                    .py_4()
                    .border_b_1()
                    .border_color(border_color.opacity(0.3))
                    .justify_center()
                    .child(
                        h_flex()
                            .w_full()
                            .max_w(rems(48.))
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(22.))
                                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                            .text_color(accent_color)
                                            .child("[/]"),
                                    )
                                    .child(
                                        v_flex()
                                            .child(
                                                div()
                                                    .text_size(px(16.))
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .child("terminal agent launcher"),
                                            )
                                            .child(
                                                Label::new("AI Command Center for Void")
                                                    .size(LabelSize::XSmall)
                                                    .color(Color::Muted),
                                            ),
                                    )
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .when(is_refreshing, |this| {
                                        this.child(
                                            Label::new("probing...")
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted)
                                        )
                                    })
                                    .child(
                                        IconButton::new("refresh", IconName::RotateCw)
                                            .icon_size(IconSize::Small)
                                            .icon_color(if is_refreshing { Color::Accent } else { Color::Muted })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.check_all_binaries(cx);
                                            }))
                                            .tooltip(|window, cx| Tooltip::text("Re-probe all binaries")(window, cx))
                                    )
                            )
                    )
            )
            // ─── Scrollable Agent List ────────────────────────────────
            .child(
                div()
                    .id("launcher-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        h_flex()
                            .justify_center()
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_w(rems(48.))
                                    .p_5()
                                    .gap_2()
                                    .children(self.agents.iter().enumerate().map(|(i, agent)| {
                                        let agent_name = agent.name;
                                        let is_installed = agent.status == AgentStatus::Installed;
                                        let is_checking = agent.status == AgentStatus::Checking;
                                        let is_not_installed = agent.status == AgentStatus::NotInstalled;
                                        let is_expanded = self.expanded_indices.contains(&i);
                                        let is_copied = self.copied_indices.contains(&i);
                                        
                                        v_flex()
                                            .w_full()
                                            .bg(surface_color.opacity(0.2))
                                            .border_1()
                                            .border_color(if is_expanded {
                                                accent_color.opacity(0.4)
                                            } else {
                                                border_color.opacity(0.4)
                                            })
                                            .rounded_md()
                                            // ── Collapsed Header Row ──────────────────
                                            .child(
                                                div()
                                                    .id(("agent-row", i))
                                                    .flex()
                                                    .w_full()
                                                    .justify_between()
                                                    .items_center()
                                                    .px_3()
                                                    .py_2()
                                                    .cursor_pointer()
                                                    .hover(|style| style.bg(cx.theme().colors().element_hover))
                                                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_expanded(i, cx)))
                                                    .child(
                                                        // Left: chevron + name + status dot
                                                        h_flex()
                                                            .gap_2()
                                                            .items_center()
                                                            .child(
                                                                Icon::new(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                                                                    .size(IconSize::XSmall)
                                                                    .color(if is_expanded { Color::Accent } else { Color::Muted })
                                                            )
                                                            .child(
                                                                Label::new(agent_name)
                                                                    .weight(gpui::FontWeight::SEMIBOLD)
                                                                    .size(LabelSize::Small)
                                                            )
                                                            // Status indicator dot
                                                            .child(
                                                                div()
                                                                    .w(px(6.))
                                                                    .h(px(6.))
                                                                    .rounded_full()
                                                                    .bg(if is_installed {
                                                                        success_color
                                                                    } else if is_checking {
                                                                        cx.theme().colors().text_muted.opacity(0.4)
                                                                    } else {
                                                                        warning_color.opacity(0.7)
                                                                    })
                                                            )
                                                    )
                                                    .child(
                                                        // Right: state label + launch button
                                                        h_flex()
                                                            .gap_2()
                                                            .items_center()
                                                            .when(is_not_installed, |this| {
                                                                this.child(
                                                                    Label::new("not installed")
                                                                        .size(LabelSize::XSmall)
                                                                        .color(Color::Accent)
                                                                )
                                                            })
                                                            .when(is_installed, |this| {
                                                                this.child(
                                                                    Button::new(format!("launch-{}", agent_name), "LAUNCH")
                                                                        .style(ButtonStyle::Filled)
                                                                        .size(ButtonSize::Compact)
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
                                            // ── Expanded Detail Panel ─────────────────
                                            .when(is_expanded, |this| {
                                                this.child(
                                                    v_flex()
                                                        .px_4()
                                                        .py_3()
                                                        .bg(cx.theme().colors().editor_background.opacity(0.5))
                                                        .border_t_1()
                                                        .border_color(border_color.opacity(0.25))
                                                        .gap_3()
                                                        // Description
                                                        .child(
                                                            div()
                                                                .text_size(px(13.))
                                                                .text_color(cx.theme().colors().text)
                                                                .child(agent.description)
                                                        )
                                                        // Requirements badge row
                                                        .child(
                                                            h_flex()
                                                                .gap_2()
                                                                .items_center()
                                                                .child(
                                                                    Icon::new(IconName::Info)
                                                                        .size(IconSize::XSmall)
                                                                        .color(Color::Muted)
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(11.))
                                                                        .font_family(SharedString::from("JetBrains Mono"))
                                                                        .text_color(cx.theme().colors().text_muted)
                                                                        .child(agent.requires)
                                                                )
                                                        )
                                                        // Install command box
                                                        .child(
                                                            h_flex()
                                                                .items_center()
                                                                .bg(cx.theme().colors().editor_background)
                                                                .px_3()
                                                                .py_2()
                                                                .rounded_md()
                                                                .border_1()
                                                                .border_color(accent_color.opacity(0.15))
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .id(("command-box", i))
                                                                        .flex_1()
                                                                        .overflow_x_scroll()
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(12.))
                                                                                .font_family(SharedString::from("JetBrains Mono"))
                                                                                .text_color(accent_color)
                                                                                .whitespace_nowrap()
                                                                                .child(agent.install_command)
                                                                        )
                                                                )
                                                                .child(
                                                                    IconButton::new(format!("copy-{}", i), if is_copied { IconName::Check } else { IconName::Copy })
                                                                        .icon_size(IconSize::XSmall)
                                                                        .icon_color(if is_copied { Color::Success } else { Color::Muted })
                                                                        .on_click(cx.listener({
                                                                            let cmd = agent.install_command.to_string();
                                                                            move |this, _, _, cx| {
                                                                                this.copy_install_command(i, cmd.clone(), cx);
                                                                            }
                                                                        }))
                                                                        .tooltip(move |window, cx| {
                                                                            Tooltip::text(if is_copied { "Copied!" } else { "Copy command" })(window, cx)
                                                                        })
                                                                )
                                                        )
                                                        // Footer row: "not installed" tip + docs link
                                                        .child(
                                                            h_flex()
                                                                .justify_between()
                                                                .items_center()
                                                                .when(is_not_installed, |this| {
                                                                    this.child(
                                                                        Label::new("↑ Copy & run in your terminal to install")
                                                                            .size(LabelSize::XSmall)
                                                                            .color(Color::Accent)
                                                                    )
                                                                })
                                                                .when(!is_not_installed, |this| this.child(div()))
                                                                .child(
                                                                    Button::new(format!("docs-{}", agent_name), "Docs →")
                                                                        .size(ButtonSize::Compact)
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
                    )
            )
            // ─── Footer ───────────────────────────────────────────────
            .child(
                h_flex()
                    .w_full()
                    .px_5()
                    .py_2()
                    .justify_between()
                    .border_t_1()
                    .border_color(border_color.opacity(0.2))
                    .child(
                        Label::new("VOID AI Terminal Hub")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                    )
                    .child(
                        // Installed count summary
                        {
                            let installed_count = self.agents.iter().filter(|a| a.status == AgentStatus::Installed).count();
                            let total = self.agents.len();
                            Label::new(format!("{}/{} installed", installed_count, total))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                        }
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
