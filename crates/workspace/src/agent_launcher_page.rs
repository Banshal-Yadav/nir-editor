use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Render, WeakEntity, Window, SharedString, StatefulInteractiveElement, InteractiveElement,
};
use ui::prelude::*;
use ui::Tooltip;
use crate::{
    OpenOptions, Workspace,
    agent_config::{AgentConfig, config_path, load_config},
    item::{Item, ItemEvent},
};
use task::{
    HideStrategy, RevealStrategy, RevealTarget, SaveStrategy, Shell, SpawnInTerminal, TaskId,
};
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime};

// ─── Runtime state per agent ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentStatus {
    Checking,
    Installed,
    NotInstalled,
}

struct AgentEntry {
    config: AgentConfig,
    status: AgentStatus,
}

// ─── Running agent terminal state ────────────────────────────────────────────

/// A record of an agent CLI launched via the Agent Launcher.
/// Used to show the "Running Agents" mini-preview without needing to scan panes.
/// The terminal itself is tracked by AgentPanel and appears in the sidebar.
struct LaunchedAgent {
    /// Display name (e.g. "Claude Code").
    name: SharedString,
    /// When the launch was initiated.
    launched_at: Instant,
}

// ─── Page state ──────────────────────────────────────────────────────────────

pub struct AgentLauncherPage {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    agents: Vec<AgentEntry>,
    expanded_indices: HashSet<usize>,
    copied_indices: HashSet<usize>,
    is_refreshing: bool,
    pending_probes: usize,
    config_error: Option<String>,
    config_last_modified: Option<SystemTime>,
    // ── Running agent preview ──────────────────────────────────────────
    /// Agents launched in this session. Source of truth for the mini-preview.
    launched_agents: Vec<LaunchedAgent>,
    preview_expanded: bool,
}

impl AgentLauncherPage {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let loaded = load_config();

        let agents = loaded
            .agents
            .into_iter()
            .map(|c| AgentEntry { config: c, status: AgentStatus::Checking })
            .collect();

        let mut this = Self {
            workspace: workspace.clone(),
            focus_handle,
            agents,
            expanded_indices: HashSet::new(),
            copied_indices: HashSet::new(),
            is_refreshing: false,
            pending_probes: 0,
            config_error: loaded.error,
            config_last_modified: loaded.modified,
            launched_agents: Vec::new(),
            preview_expanded: false,
        };

        this.check_all_binaries(cx);

        // Background: probe every 15s, reload config, and expire stale launched_agents
        // entries. Entries older than 10 minutes are removed so the mini-preview badge
        // doesn't linger after the terminal is closed. The sidebar (driven by AgentPanel)
        // is the authoritative running-agents view.
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor().timer(Duration::from_secs(15)).await;
                    let ok = this
                        .update(&mut cx, |this, cx| {
                            this.check_config_reload(cx);
                            this.check_all_binaries(cx);
                            // Expire launched_agents entries older than 10 minutes.
                            let before = this.launched_agents.len();
                            this.launched_agents.retain(|a| {
                                a.launched_at.elapsed() < Duration::from_secs(10 * 60)
                            });
                            if this.launched_agents.len() != before {
                                cx.notify();
                            }
                        })
                        .is_ok();
                    if !ok {
                        break;
                    }
                }
            }
        })
        .detach();

        this
    }

    // ── Config hot-reload ─────────────────────────────────────────────────────

    fn check_config_reload(&mut self, cx: &mut Context<Self>) {
        let path = config_path();
        let current_mod = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        if current_mod != self.config_last_modified {
            self.reload_config(cx);
        }
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        let loaded = load_config();
        self.config_error = loaded.error;
        self.config_last_modified = loaded.modified;
        self.agents = loaded
            .agents
            .into_iter()
            .map(|c| AgentEntry { config: c, status: AgentStatus::Checking })
            .collect();
        self.expanded_indices.clear();
        cx.notify();
        self.check_all_binaries(cx);
    }

    // ── Binary probing ────────────────────────────────────────────────────────

    fn check_all_binaries(&mut self, cx: &mut Context<Self>) {
        self.is_refreshing = true;
        self.pending_probes = self.agents.len();
        cx.notify();

        for (i, entry) in self.agents.iter().enumerate() {
            let binary = entry.config.probe_binary().to_string();
            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let installed = check_binary(&binary).await;
                    let _ = this.update(&mut cx, |this, cx: &mut Context<Self>| {
                        if i < this.agents.len() {
                            this.agents[i].status = if installed {
                                AgentStatus::Installed
                            } else {
                                AgentStatus::NotInstalled
                            };
                        }
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

    // ── Actions ───────────────────────────────────────────────────────────────

    fn launch_agent(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.agents.get(index) else { return };
        let command = entry.config.launch_cmd.clone();
        let name = entry.config.name.clone();
        let agent_id = entry.config.id.clone();

        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                // Compute the working directory here while we have &mut Workspace directly
                // (without needing to read-lease the entity). This prevents the double-lease
                // panic that occurs when AgentPanel::new_terminal_with_task() later tries to
                // call default_terminal_working_directory() → workspace.read(cx) while
                // workspace is still mutably leased in this update closure.
                let cwd: Option<std::path::PathBuf> = workspace
                    .worktrees(cx)
                    .next()
                    .map(|wt| wt.read(cx).abs_path().to_path_buf())
                    .or_else(|| {
                        std::env::var("HOME")
                            .or_else(|_| std::env::var("USERPROFILE"))
                            .ok()
                            .map(std::path::PathBuf::from)
                    });
                // On Windows, agent CLI tools (claude, opencode, aider, etc.) are installed as
                // .cmd batch scripts by npm/npx and cannot be spawned directly as a process —
                // they must be run via `cmd.exe /c <command>`. On other platforms the binary
                // can be invoked directly.
                #[cfg(target_os = "windows")]
                let (final_command, final_args) = (
                    "cmd.exe".to_string(),
                    vec![
                        "/c".to_string(),
                        format!("set PATH=%APPDATA%\\npm;%ProgramFiles%\\nodejs;%PATH% && {}", command),
                    ],
                );
                #[cfg(not(target_os = "windows"))]
                let (final_command, final_args) = (command.clone(), vec![]);
                let spawn_task = SpawnInTerminal {
                    id: TaskId(format!("agent-{}", name.to_lowercase().replace(' ', "-"))),
                    full_label: format!("Launch {name}"),
                    label: name.clone(),
                    command: Some(final_command),
                    args: final_args,
                    command_label: command.clone(),
                    cwd,
                    // SpawnInTerminal.env is merged (extend) onto the resolved system
                    // environment in terminals.rs — empty means "inherit everything".
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
                // Route through AgentPanel (via the registered AgentTerminalSpawner) so the
                // terminal gets a TerminalId, appears in the sidebar, and persists across
                // sessions. Falls back to spawn_in_terminal if AgentPanel is not active.
                workspace.spawn_agent_terminal(
                    spawn_task,
                    Some(command),         // launch_cmd - full command string for restore
                    Some(agent_id.into()), // agent_icon - id slug (e.g. "claude-code")
                    window,
                    cx,
                );
            });
        }

        // Track the launch locally so the mini-preview can show a "running" badge.
        // The terminal itself is tracked by AgentPanel and shown in the sidebar.
        let name_shared: SharedString = name.into();
        if !self.launched_agents.iter().any(|a| a.name == name_shared) {
            self.launched_agents.push(LaunchedAgent {
                name: name_shared,
                launched_at: Instant::now(),
            });
            cx.notify();
        }
    }

    // ── Running agents preview ────────────────────────────────────────────────

    /// Render the running agents preview card at the top of the launcher.
    fn render_running_agents(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let border_color = cx.theme().colors().border;
        let is_empty = self.launched_agents.is_empty();
        let active_count = self.launched_agents.len();

        Some(
            h_flex()
                .w_full()
                .justify_center()
                .child(
                    v_flex()
                        .w_full()
                        .max_w(rems(48.))
                        .p_5()
                        .child(
                            v_flex()
                                .w_full()
                                .rounded_lg()
                                .bg(cx.theme().colors().surface_background)
                                .border_1()
                                .border_color(border_color.opacity(0.5))
                                .child(
                                    h_flex()
                                        .id("running-agents-header-row")
                                        .w_full()
                                        .justify_between()
                                        .items_center()
                                        .px_3()
                                        .py_2p5()
                                        .when(is_empty, |this| {
                                            this.child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .w(px(8.))
                                                            .h(px(8.))
                                                            .rounded_full()
                                                            .bg(cx.theme().colors().icon_muted.opacity(0.4)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .text_color(cx.theme().colors().text_muted)
                                                            .child("RUNNING AGENTS • 0 active"),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(cx.theme().colors().text_muted.opacity(0.6))
                                                    .child("launch one below to start"),
                                            )
                                        })
                                        .when(!is_empty, |this| {
                                            this.cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.preview_expanded = !this.preview_expanded;
                                                    cx.notify();
                                                }))
                                                .child(
                                                    h_flex()
                                                        .gap_2()
                                                        .items_center()
                                                        .child(
                                                            div()
                                                                .w(px(8.))
                                                                .h(px(8.))
                                                                .rounded_full()
                                                                .bg(cx.theme().colors().icon_accent.opacity(0.8)),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(11.))
                                                                .font_weight(gpui::FontWeight::BOLD)
                                                                .text_color(cx.theme().colors().text)
                                                                .child("RUNNING AGENTS"),
                                                        )
                                                        .child(
                                                            div()
                                                                .px_2()
                                                                .py(px(1.))
                                                                .rounded_full()
                                                                .bg(cx.theme().colors().editor_background.opacity(0.5))
                                                                .border_1()
                                                                .border_color(border_color.opacity(0.3))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(10.))
                                                                        .text_color(cx.theme().colors().text_muted)
                                                                        .child(format!("{} active", active_count)),
                                                                ),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(cx.theme().colors().text_muted)
                                                        .hover(|s| s.text_color(cx.theme().colors().text))
                                                        .child(if self.preview_expanded { "collapse" } else { "expand" }),
                                                )
                                        }),
                                )
                                .when(!is_empty && self.preview_expanded, |this| {
                                    this.children(self.launched_agents.iter().map(|agent| {
                                        Self::render_agent_row(agent, cx)
                                    }))
                                }),
                        ),
                ),
        )
    }

    /// Render a single launched agent row showing its name and sidebar status.
    fn render_agent_row(agent: &LaunchedAgent, cx: &Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().colors().border;
        let accent = cx.theme().colors().text_accent;

        let first_char = agent
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "A".to_string());

        let status_line: SharedString = if agent.launched_at.elapsed().as_secs() < 5 {
            "Starting…".into()
        } else {
            "Running — visible in sidebar".into()
        };

        v_flex()
            .w_full()
            .border_t_1()
            .border_color(border_color.opacity(0.2))
            .child(
                h_flex()
                    .id(format!("agent-row-compact-{}", agent.name))
                    .w_full()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_2p5()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                div()
                                    .w(px(28.))
                                    .h(px(28.))
                                    .rounded_md()
                                    .bg(cx.theme().colors().editor_background)
                                    .border_1()
                                    .border_color(accent.opacity(0.15))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(cx.theme().status().success)
                                            .child(first_char),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .gap(px(1.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().colors().text)
                                            .child(agent.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(cx.theme().status().success)
                                            .child(status_line),
                                    ),
                            ),
                    )
            )
    }

    fn open_config_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = config_path();
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_abs_path(path, OpenOptions::default(), window, cx)
                    .detach();
            });
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
        })
        .detach();
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

// ─── GPUI traits ─────────────────────────────────────────────────────────────

impl EventEmitter<ItemEvent> for AgentLauncherPage {}

impl Focusable for AgentLauncherPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentLauncherPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Scan is done by the 300ms polling loop to avoid read-during-update panics.
        // render() must NOT read workspace/pane entities — they may be leased by another update.

        let border_color = cx.theme().colors().border;
        let accent = cx.theme().colors().text_accent;
        let surface = cx.theme().colors().elevated_surface_background;
        let success = cx.theme().status().success;
        let is_refreshing = self.is_refreshing;
        let has_error = self.config_error.is_some();
        let error_msg = self.config_error.clone().unwrap_or_default();
        let installed_count =
            self.agents.iter().filter(|a| a.status == AgentStatus::Installed).count();
        let total = self.agents.len();

        div()
            .id("agent-launcher-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().colors().editor_background)
            // ── Header ───────────────────────────────────────────────
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
                                            .text_color(accent)
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
                                                Label::new("AI Command Center for /nir")
                                                    .size(LabelSize::XSmall)
                                                    .color(Color::Muted),
                                            ),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .when(is_refreshing, |this| {
                                        this.child(
                                            Label::new("probing...")
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted),
                                        )
                                    })
                                    .child(
                                        IconButton::new("refresh", IconName::RotateCw)
                                            .icon_size(IconSize::Small)
                                            .icon_color(if is_refreshing {
                                                Color::Accent
                                            } else {
                                                Color::Muted
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.check_all_binaries(cx);
                                            }))
                                            .tooltip(|window, cx| {
                                                Tooltip::text("Re-probe all binaries")(window, cx)
                                            }),
                                    ),
                            ),
                    ),
            )
            // ── Config error banner ───────────────────────────────────
            .when(has_error, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .px_5()
                        .py_2()
                        .bg(cx.theme().colors().editor_background)
                        .border_b_1()
                        .border_color(border_color.opacity(0.3))
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Icon::new(IconName::Warning)
                                        .size(IconSize::XSmall)
                                        .color(Color::Muted),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(cx.theme().colors().text_muted)
                                        .child(error_msg),
                                ),
                        )
                        .child(
                            Label::new("Fix the error and save, then reopen the launcher to reload.")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
            })
            // ── Scrollable content area ───────────────────────────────
            .child(
                div()
                    .id("launcher-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .w_full()
                            // ── Running agents preview ─────────────────────────────────
                            .children(self.render_running_agents(cx))
                            // ── Divider between preview and agent list ──────────────────
                            .when(self.render_running_agents(cx).is_some(), |this| {
                                this.child(
                                    div()
                                        .w_full()
                                        .h_px()
                                        .bg(border_color.opacity(0.3)),
                                )
                            })
                            // ── Agent list ─────────────────────────────────
                            .child(
                                h_flex().justify_center().child(
                            v_flex()
                                .w_full()
                                .max_w(rems(48.))
                                .p_5()
                                .gap_2()
                                .children(self.agents.iter().enumerate().map(|(i, entry)| {
                                    let is_installed = entry.status == AgentStatus::Installed;
                                    let is_not_installed =
                                        entry.status == AgentStatus::NotInstalled;
                                    let is_expanded = self.expanded_indices.contains(&i);
                                    let is_copied = self.copied_indices.contains(&i);
                                    let name = entry.config.name.clone();
                                    let description = entry.config.description.clone();
                                    let requires = entry.config.requires.clone();
                                    let install_cmd = entry.config.install_cmd.clone();
                                    let docs_url = entry.config.docs_url.clone();

                                    v_flex()
                                        .w_full()
                                        .bg(surface.opacity(0.3))
                                        .border_1()
                                        .border_color(if is_expanded { accent.opacity(0.5) } else { border_color.opacity(0.3) })
                                        .rounded_lg()
                                        .overflow_hidden()
                                        .hover(|s| s.border_color(if is_expanded { accent.opacity(0.6) } else { accent.opacity(0.3) }))
                                        // Collapsed row
                                        .child(
                                            div()
                                                .id(("agent-row", i))
                                                .flex()
                                                .w_full()
                                                .justify_between()
                                                .items_center()
                                                .px_3()
                                                .py_3()
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(cx.theme().colors().element_hover)
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, _, cx| {
                                                        this.toggle_expanded(i, cx);
                                                    },
                                                ))
                                                .child(
                                                    h_flex()
                                                        .gap_3()
                                                        .items_center()
                                                        // 1. Sleek Avatar Badge
                                                        .child(
                                                            div()
                                                                .w(px(34.))
                                                                .h(px(34.))
                                                                .rounded_md()
                                                                .bg(cx.theme().colors().editor_background)
                                                                .border_1()
                                                                .border_color(accent.opacity(0.15))
                                                                .flex().items_center().justify_center()
                                                                .child(
                                                                    div()
                                                                        .text_size(px(15.))
                                                                        .font_weight(gpui::FontWeight::BOLD)
                                                                        .text_color(if is_installed { success } else { cx.theme().colors().text_muted })
                                                                        .child(name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "A".to_string()))
                                                                )
                                                        )
                                                        // 2. Name and short description
                                                        .child(
                                                            v_flex()
                                                                .child(
                                                                    div()
                                                                        .text_size(px(14.))
                                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                                        .text_color(cx.theme().colors().text)
                                                                        .child(name.clone())
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.))
                                                                        .text_color(cx.theme().colors().text_muted)
                                                                        .child(if description.chars().count() > 45 { format!("{}...", description.chars().take(45).collect::<String>()) } else { description.clone() })
                                                                )
                                                        )
                                                )
                                                .child(
                                                    h_flex()
                                                        .gap_4()
                                                        .items_center()
                                                        // 3. Status Pill / Launch Button
                                                        .when(is_not_installed, |this| {
                                                            this.child(
                                                                div()
                                                                    .px_2().py_1()
                                                                    .rounded_md()
                                                                    .bg(cx.theme().colors().element_background)
                                                                    .border_1().border_color(border_color.opacity(0.6))
                                                                    .child(
                                                                        div().text_size(px(10.)).font_weight(gpui::FontWeight::MEDIUM).text_color(cx.theme().colors().text_muted).child("NOT INSTALLED")
                                                                    )
                                                            )
                                                        })
                                                        .when(is_installed, |this| {
                                                            this.child(
                                                                div()
                                                                    .id(format!("launch-btn-{i}"))
                                                                    .cursor_pointer()
                                                                    .px_3()
                                                                    .py_1()
                                                                    .rounded_md()
                                                                    .bg(success.opacity(0.1))
                                                                    .border_1()
                                                                    .border_color(success.opacity(0.4))
                                                                    .hover(|s| s.bg(success.opacity(0.2)))
                                                                    .child(
                                                                        h_flex()
                                                                            .gap_1p5()
                                                                            .items_center()
                                                                            .child(
                                                                                div().w(px(5.)).h(px(5.)).rounded_full().bg(success)
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_size(px(11.))
                                                                                    .font_weight(gpui::FontWeight::BOLD)
                                                                                    .text_color(success)
                                                                                    .child("LAUNCH"),
                                                                            ),
                                                                    )
                                                                    .on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.launch_agent(i, window, cx);
                                                                        },
                                                                    ))
                                                            )
                                                        })
                                                        // Chevron
                                                        .child(
                                                            Icon::new(if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                                                                .size(IconSize::Small)
                                                                .color(Color::Muted)
                                                        )
                                                ),
                                        )
                                        // Expanded panel
                                        .when(is_expanded, |this| {
                                            this.child(
                                                v_flex()
                                                    .px_4()
                                                    .py_3()
                                                    .bg(cx
                                                        .theme()
                                                        .colors()
                                                        .editor_background
                                                        .opacity(0.5))
                                                    .border_t_1()
                                                    .border_color(border_color.opacity(0.25))
                                                    .gap_3()
                                                    // Description
                                                    .child(
                                                        div()
                                                            .text_size(px(13.))
                                                            .text_color(
                                                                cx.theme().colors().text,
                                                            )
                                                            .child(description),
                                                    )
                                                    // Requirements badge
                                                    .when(!requires.is_empty(), |this| {
                                                        this.child(
                                                            h_flex()
                                                                .gap_2()
                                                                .items_center()
                                                                .child(
                                                                    Icon::new(IconName::Info)
                                                                        .size(IconSize::XSmall)
                                                                        .color(Color::Muted),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(11.))
                                                                        .font_family(
                                                                            SharedString::from(
                                                                                "JetBrains Mono",
                                                                            ),
                                                                        )
                                                                        .text_color(
                                                                            cx.theme()
                                                                                .colors()
                                                                                .text_muted,
                                                                        )
                                                                        .child(requires),
                                                                ),
                                                        )
                                                    })
                                                    // Install command box
                                                    .when(!install_cmd.is_empty(), |this| {
                                                        this.child(
                                                            h_flex()
                                                                .items_center()
                                                                .bg(cx
                                                                    .theme()
                                                                    .colors()
                                                                    .editor_background)
                                                                .px_3()
                                                                .py_2()
                                                                .rounded_md()
                                                                .border_1()
                                                                .border_color(
                                                                    accent.opacity(0.15),
                                                                )
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .id(("cmd-box", i))
                                                                        .flex_1()
                                                                        .overflow_x_scroll()
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(12.))
                                                                                .font_family(
                                                                                    SharedString::from("JetBrains Mono"),
                                                                                )
                                                                                .text_color(accent)
                                                                                .whitespace_nowrap()
                                                                                .child(
                                                                                    install_cmd
                                                                                        .clone(),
                                                                                ),
                                                                        ),
                                                                )
                                                                .child(
                                                                    IconButton::new(
                                                                        format!("copy-{i}"),
                                                                        if is_copied {
                                                                            IconName::Check
                                                                        } else {
                                                                            IconName::Copy
                                                                        },
                                                                    )
                                                                    .icon_size(IconSize::XSmall)
                                                                    .icon_color(if is_copied {
                                                                        Color::Success
                                                                    } else {
                                                                        Color::Muted
                                                                    })
                                                                    .on_click(cx.listener({
                                                                        let cmd =
                                                                            install_cmd.clone();
                                                                        move |this, _, _, cx| {
                                                                            this.copy_install_command(
                                                                                i,
                                                                                cmd.clone(),
                                                                                cx,
                                                                            );
                                                                        }
                                                                    }))
                                                                    .tooltip(move |window, cx| {
                                                                        Tooltip::text(
                                                                            if is_copied {
                                                                                "Copied!"
                                                                            } else {
                                                                                "Copy command"
                                                                            },
                                                                        )(window, cx)
                                                                    }),
                                                                ),
                                                        )
                                                    })
                                                    // Footer: hint + docs
                                                    .child(
                                                        h_flex()
                                                            .justify_between()
                                                            .items_center()
                                                            .when(is_not_installed, |this| {
                                                                this.child(
                                                                    Label::new(
                                                                        "↑ Copy & run in terminal to install",
                                                                    )
                                                                    .size(LabelSize::XSmall)
                                                                    .color(Color::Accent),
                                                                )
                                                            })
                                                            .when(!is_not_installed, |this| {
                                                                this.child(div())
                                                            })
                                                            .when(!docs_url.is_empty(), |this| {
                                                                this.child(
                                                                    Button::new(
                                                                        format!("docs-{i}"),
                                                                        "Docs →",
                                                                    )
                                                                    .size(ButtonSize::Compact)
                                                                    .style(ButtonStyle::Subtle)
                                                                    .on_click({
                                                                        let url = docs_url.clone();
                                                                        move |_, _, cx| {
                                                                            cx.open_url(&url);
                                                                        }
                                                                    }),
                                                                )
                                                            }),
                                                    ),
                                            )
                                        })
                                })),
                            ),
                        ),
                    ),
            )
            // ── Footer ────────────────────────────────────────────────
            .child(
                h_flex()
                    .w_full()
                    .px_5()
                    .py_2()
                    .justify_between()
                    .items_center()
                    .border_t_1()
                    .border_color(border_color.opacity(0.2))
                    .child(
                        Button::new("edit-config", "terminal-agents.json")
                            .size(ButtonSize::Compact)
                            .style(ButtonStyle::Subtle)
                            .start_icon(Icon::new(IconName::Settings).size(IconSize::XSmall))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_config_file(window, cx);
                            }))
                            .tooltip(|window, cx| {
                                Tooltip::text("Open config in editor — save to hot-reload")(
                                    window, cx,
                                )
                            }),
                    )
                    .child(
                        Label::new(format!("{installed_count}/{total} installed"))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

// ─── Item impl ────────────────────────────────────────────────────────────────

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
        gpui::Task::ready(Some(
            cx.new(|cx| AgentLauncherPage::new(self.workspace.clone(), cx)),
        ))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

