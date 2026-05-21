use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Render, Subscription, WeakEntity, Window, SharedString, StatefulInteractiveElement, InteractiveElement,
};
use terminal::Terminal;
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

/// A terminal agent launched via the Agent Launcher, tracked for the mini preview.
struct RunningAgent {
    /// Display name (e.g. "Claude Code").
    name: SharedString,
    /// Weak handle to the Terminal entity.
    terminal: WeakEntity<Terminal>,
    /// Up to 8 lines of recent output (unfiltered, shows TUI + response).
    output_lines: Vec<SharedString>,
    /// The single filtered meaningful line for collapsed view.
    filtered_line: Option<SharedString>,
    /// Whether the task is still running.
    is_active: bool,
    /// Whether this agent's output is currently expanded in the mini preview.
    is_expanded: bool,
    /// When this entry was last updated.
    last_update: Instant,
    /// When the output buffer last changed. Used to distinguish an actively streaming/responding agent from an idle/waiting session.
    last_output_change: Instant,
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
    running_agents: Vec<RunningAgent>,
    _terminal_subscriptions: Vec<Subscription>,
    _workspace_observe: Option<Subscription>,
    last_agent_scan: Instant,
    agent_scan_initialized: bool,
    initial_scan_scheduled: bool,
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
            workspace,
            focus_handle,
            agents,
            expanded_indices: HashSet::new(),
            copied_indices: HashSet::new(),
            is_refreshing: false,
            pending_probes: 0,
            config_error: loaded.error,
            config_last_modified: loaded.modified,
            running_agents: Vec::new(),
            _terminal_subscriptions: Vec::new(),
            _workspace_observe: None,
            last_agent_scan: Instant::now(),
            agent_scan_initialized: false,
            initial_scan_scheduled: true,
            preview_expanded: false,
        };

        // Set up workspace observation for terminal agent detection.
        // Must be done during construction — the workspace isn't under a lease here.
        if let Some(strong) = this.workspace.upgrade() {
            let _weak = this.workspace.clone();
            this._workspace_observe = Some(cx.observe(&strong, move |this, _, _cx| {
                this.agent_scan_initialized = true;
                // The actual scan happens lazily in render() to avoid
                // read-during-update panics.
            }));
        }

        this.check_all_binaries(cx);

        // Background: probe every 15s and watch for config changes
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor().timer(Duration::from_secs(15)).await;
                    let ok = this
                        .update(&mut cx, |this, cx| {
                            this.check_config_reload(cx);
                            this.check_all_binaries(cx);
                        })
                        .is_ok();
                    if !ok {
                        break;
                    }
                }
            }
        })
        .detach();

        // Fast live streaming poller: poll running agents output every 300ms for real-time mini preview updates.
        // Also handles the initial terminal scan (deferred to avoid read-during-update panics).
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                // Initial scan after 500ms delay — avoids lease conflicts during construction
                cx.background_executor().timer(Duration::from_millis(500)).await;
                this.update(&mut cx, |this, cx| {
                    if this.initial_scan_scheduled {
                        this.initial_scan_scheduled = false;
                        this.update_running_agents(cx);
                    }
                })
                .ok();

                // Polling loop
                loop {
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                    let ok = this
                        .update(&mut cx, |this, cx| {
                            // Re-scan when workspace observer signals a change
                            if this.agent_scan_initialized {
                                this.agent_scan_initialized = false;
                                this.update_running_agents(cx);
                            }
                            this.poll_running_agents(cx);
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

        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                let action = SpawnInTerminal {
                    id: TaskId(format!("agent-{}", name.to_lowercase().replace(' ', "-"))),
                    full_label: format!("Launch {name}"),
                    label: name.clone(),
                    command: Some(command.clone()),
                    args: vec![],
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
        }
    }

    // ── Running agents preview ────────────────────────────────────────────────

    /// Scan workspace panes for agent terminals, subscribe to new ones, and
    /// update running_agents state. Called from render() to avoid lease conflicts.
    fn update_running_agents(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        // Debounce: don't scan more than once per 500ms
        if now.duration_since(self.last_agent_scan) < Duration::from_millis(500) {
            return;
        }
        self.last_agent_scan = now;

        let Some(workspace) = self.workspace.upgrade() else { return };
        let panes = workspace.read(cx).panes().to_vec();

        // Collect all active agent names and their latest terminal entities from all panes.
        let self_id = cx.entity().entity_id();
        let mut active_scanned: Vec<(SharedString, Entity<Terminal>)> = Vec::new();

        for pane in &panes {
            let items: Vec<_> = pane.read(cx).items().cloned().collect();
            for item in items {
                if item.item_id() == self_id {
                    continue;
                }
                let Some(terminal) = item.act_as_terminal(cx) else { continue };
                let task_info = terminal.read_with(cx, |t, _| {
                    t.task().map(|task| {
                        let is_agent = task.spawned_task.id.0.starts_with("agent-");
                        let name: SharedString = if !task.spawned_task.full_label.is_empty() {
                            task.spawned_task.full_label.trim_start_matches("Launch ").to_string().into()
                        } else {
                            task.spawned_task.id.0.trim_start_matches("agent-").to_string().into()
                        };
                        (is_agent, name)
                    })
                });
                if let Some((true, name)) = task_info {
                    // Only keep the first one found if duplicate views exist for the same agent task
                    if !active_scanned.iter().any(|(n, _)| n == &name) {
                        active_scanned.push((name, terminal));
                    }
                }
            }
        }

        // Now update self.running_agents to match active_scanned while strictly preserving insertion order and expansion states!
        // First, retain only those running_agents whose names are present in active_scanned.
        self.running_agents.retain(|agent| active_scanned.iter().any(|(n, _)| n == &agent.name));

        // For each scanned active agent, either update its dead terminal handle in-place or append a fresh entry.
        for (name, terminal) in active_scanned {
            if let Some(existing) = self.running_agents.iter_mut().find(|a| a.name == name) {
                // If the terminal view entity changed (e.g. moved to side tab), update handle and resubscribe!
                if existing.terminal.upgrade().map_or(true, |t| t != terminal) {
                    existing.terminal = terminal.downgrade();
                    let weak_term = terminal.downgrade();
                    let sub = cx.subscribe(
                        &terminal,
                        move |this, _term, event, cx| {
                            if let terminal::Event::Wakeup = event {
                                if let Some(terminal) = weak_term.upgrade() {
                                    this.on_agent_output(&terminal, cx);
                                }
                            }
                        },
                    );
                    self._terminal_subscriptions.push(sub);
                }
            } else {
                // Brand new agent opened! Append to the end to strictly preserve chronological insertion order.
                let weak_term = terminal.downgrade();
                let sub = cx.subscribe(
                    &terminal,
                    move |this, _term, event, cx| {
                        if let terminal::Event::Wakeup = event {
                            if let Some(terminal) = weak_term.upgrade() {
                                this.on_agent_output(&terminal, cx);
                            }
                        }
                    },
                );
                self._terminal_subscriptions.push(sub);

                self.running_agents.push(RunningAgent {
                    name,
                    terminal: terminal.downgrade(),
                    output_lines: Vec::new(),
                    filtered_line: None,
                    is_active: true,
                    is_expanded: false,
                    last_update: Instant::now(),
                    last_output_change: Instant::now(),
                });
                cx.notify();
            }
        }
    }

    /// Called when a tracked agent terminal emits Wakeup — reads the terminal
    /// output and updates both filtered (collapsed) and raw (expanded) lines.
    fn on_agent_output(
        &mut self,
        terminal: &Entity<Terminal>,
        cx: &mut Context<Self>,
    ) {
        let (name, filtered, raw_lines, task_running) = terminal.read_with(cx, |t, _| {
            let name = t
                .task()
                .map(|task| {
                    if !task.spawned_task.full_label.is_empty() {
                        task.spawned_task
                            .full_label
                            .trim_start_matches("Launch ")
                            .to_string()
                    } else {
                        task.spawned_task
                            .id
                            .0
                            .trim_start_matches("agent-")
                            .to_string()
                    }
                })
                .unwrap_or_default();
            let filtered = extract_meaningful_output(t);
            let raw_lines = extract_raw_output(t, 8);
            let task_running =
                t.task().is_some_and(|task| task.status == terminal::TaskStatus::Running);
            (name, filtered, raw_lines, task_running)
        });

        if let Some(agent) = self
            .running_agents
            .iter_mut()
            .find(|a| a.name == name)
        {
            if agent.output_lines != raw_lines {
                agent.last_output_change = Instant::now();
            }
            agent.output_lines = raw_lines;
            agent.filtered_line = filtered;
            agent.is_active = task_running && agent.last_output_change.elapsed() < Duration::from_secs(2);
            agent.last_update = Instant::now();
        }
        cx.notify();
    }

    /// Poll all tracked running agents to stream live terminal responses.
    fn poll_running_agents(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for agent in &mut self.running_agents {
            if let Some(term) = agent.terminal.upgrade() {
                let (filtered, raw_lines, task_running) = term.read_with(cx, |t, _| {
                    let filtered = extract_meaningful_output(t);
                    let raw_lines = extract_raw_output(t, 8);
                    let task_running = t.task().is_some_and(|task| task.status == terminal::TaskStatus::Running);
                    (filtered, raw_lines, task_running)
                });
                if agent.output_lines != raw_lines {
                    agent.last_output_change = Instant::now();
                    agent.output_lines = raw_lines;
                    agent.filtered_line = filtered;
                    changed = true;
                }
                let is_active = task_running && agent.last_output_change.elapsed() < Duration::from_secs(2);
                if agent.is_active != is_active {
                    agent.is_active = is_active;
                    changed = true;
                }
                agent.last_update = Instant::now();
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Render the running agents preview card at the top of the launcher.
    fn render_running_agents(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let border_color = cx.theme().colors().border;
        let active_count = self.running_agents.iter().filter(|a| a.is_active).count();
        let is_empty = self.running_agents.is_empty();

        let sorted: Vec<&RunningAgent> = self.running_agents.iter().collect();

        Some(
            // ── Centered wrapper with max width matching below list EXACTLY ──
            h_flex()
                .w_full()
                .justify_center()
                .child(
                    v_flex()
                        .w_full()
                        .max_w(rems(48.))
                        .p_5() // Matches below list boundary box exactly!
                        .child(
                            // Outer Card Container matching below agent list exactly!
                            v_flex()
                                .w_full()
                                .rounded_lg()
                                .bg(cx.theme().colors().surface_background)
                                .border_1()
                                .border_color(border_color.opacity(0.5))
                                .child(
                                    // Top header row: idle vs active views matching screenshot
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
                                                                .bg(if active_count > 0 {
                                                                    cx.theme().colors().icon_accent.opacity(0.8)
                                                                } else {
                                                                    cx.theme().colors().icon_muted.opacity(0.4)
                                                                }),
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
                                                        .child(if self.preview_expanded {
                                                            "collapse"
                                                        } else {
                                                            "expand"
                                                        }),
                                                )
                                        }),
                                )
                                // ── Expandable agent items below header line ──────────────────────────
                                .when(!is_empty && self.preview_expanded, |this| {
                                    this.children(sorted.into_iter().map(|agent| {
                                        Self::render_agent_row(agent, cx)
                                    }))
                                }),
                        ),
                ),
        )
    }

    /// Render a single running agent compact block that expands to reveal streaming console output.
    fn render_agent_row(agent: &RunningAgent, cx: &Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().colors().border;
        let accent = cx.theme().colors().text_accent;

        let output_lines = if agent.output_lines.is_empty() {
            if let Some(ref filtered) = agent.filtered_line {
                vec![filtered.clone()]
            } else {
                vec![SharedString::from("waiting for output...")]
            }
        } else {
            agent.output_lines.clone()
        };

        let latest_line = agent
            .filtered_line
            .clone()
            .unwrap_or_else(|| SharedString::from("waiting for output..."));

        let first_char = agent
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "A".to_string());

        let name = agent.name.clone();
        let is_expanded = agent.is_expanded;

        v_flex()
            .w_full()
            .border_t_1()
            .border_color(border_color.opacity(0.2))
            .child(
                // Compact row: Avatar + stacked Name & Response + right Arrow icon
                h_flex()
                    .id(format!("agent-row-compact-{}", agent.name))
                    .w_full()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_2p5()
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().colors().element_hover))
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| {
                            if let Some(a) = this.running_agents.iter_mut().find(|a| a.name == name) {
                                a.is_expanded = !a.is_expanded;
                                cx.notify();
                            }
                        }
                    }))
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                // Sleek dark square avatar badge
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
                                // Stacked text block
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
                                            .text_color(cx.theme().status().success) // Bright green response line
                                            .child(latest_line),
                                    ),
                            ),
                    )
            )
            // ── Output lines box inside when further expanded ──
            .when(is_expanded, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .child(
                            h_flex()
                                .w_full()
                                .rounded_md()
                                .overflow_hidden()
                                .border_1()
                                .border_color(border_color.opacity(0.3))
                                .items_stretch()
                                .child(
                                    // Left accent bar reflecting active status
                                    div()
                                        .w(px(2.))
                                        .bg(if agent.is_active {
                                            cx.theme().status().success
                                        } else {
                                            border_color.opacity(0.5)
                                        }),
                                )
                                .child(
                                    // Console output area
                                    v_flex()
                                        .flex_1()
                                        .bg(cx.theme().colors().editor_background)
                                        .py_2()
                                        .px_3()
                                        .children(output_lines.iter().map(|line| {
                                            div()
                                                .text_size(px(11.))
                                                .text_color(if agent.is_active {
                                                    cx.theme().colors().text
                                                } else {
                                                    cx.theme().colors().text_muted
                                                })
                                                .child(line.clone())
                                        })),
                                ),
                        ),
                )
            })
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

// ─── Helpers: extract output lines from terminal content ─────────────────────

/// Noise-line patterns to skip — TUI chrome, shell prompts, progress bars, etc.


/// Heuristically evaluates whether a terminal output line contains genuine agent response text
/// by stripping pure borders, status indicators, short stubs, and enforcing a >50% alphanumeric rule.
fn is_meaningful_line(line: &str) -> bool {
    let s = line.trim();
    // 6. Lines that are just whitespace or empty
    // 5. Lines shorter than 4 characters
    if s.len() < 4 {
        return false;
    }

    // 1. Lines that are purely box-drawing/border characters
    if s.chars().all(|c| matches!(c, '│' | '─' | '┼' | '+' | '|' | '=' | '-' | '_' | '━' | '═' | ' ' | '\t')) {
        return false;
    }

    // Also strip lines containing continuous separator sequences or block fills
    if s.contains("──") || s.contains("━━") || s.contains("══") || s.contains("___") || s.contains('█') {
        return false;
    }

    // 2. Lines matching status bar patterns like "model:", "directory:", "workspace:", "sandbox:", "quota:"
    let lower = s.to_lowercase();
    if lower.contains("model:")
        || lower.contains("directory:")
        || lower.contains("workspace:")
        || lower.contains("sandbox")
        || lower.contains("quota:")
        || lower.contains("/model to change")
        || lower.contains("shift+tab")
        || (lower.contains("gemini") && lower.contains("file"))
        || lower.contains("type your message")
        || lower.contains("improve documentation")
        || lower.starts_with("tip:")
        || lower.starts_with("~/")
        || lower.starts_with(".\\")
        || lower.starts_with("./")
        || lower.starts_with("workspace ")
        || lower.contains("flash-preview")
        || lower.starts_with("gpt-")
        || lower.starts_with("claude-")
        || lower.starts_with("gemini-")
    {
        return false;
    }

    // 3. Lines with mostly punctuation/symbols and little actual text
    // Simple heuristic: if a line has more than 50% alphanumeric characters → show it. Otherwise skip.
    // Strip leading conversational non-alphanumeric noise (like bullet points or query prompts) before calculating ratio.
    let cleaned = s.trim_start_matches(|c: char| !c.is_alphanumeric());
    let total_chars = cleaned.chars().count();
    if total_chars == 0 {
        return false;
    }
    let alnum_chars = cleaned.chars().filter(|c| c.is_alphanumeric()).count();
    if (alnum_chars as f64) / (total_chars as f64) <= 0.5 {
        return false;
    }

    true
}

/// Extract the last meaningful line of output from a terminal, filtering out
/// TUI noise, shell prompts, and progress indicators — preferring code/text.
fn extract_meaningful_output(terminal: &Terminal) -> Option<SharedString> {
    let raw_lines = terminal.last_n_non_empty_lines(20);
    let last_non_empty = raw_lines.last().cloned();

    // Filter: scan from bottom, return first line that passes is_meaningful_line.
    for line in raw_lines.into_iter().rev() {
        if is_meaningful_line(&line) {
            let line = line.trim().to_string();
            let truncated = if line.chars().count() > 80 {
                format!("{}…", line.chars().take(80).collect::<String>())
            } else {
                line
            };
            return Some(truncated.into());
        }
    }

    // Fallback: return the last non-empty line.
    last_non_empty.map(|l| {
        let l = l.trim().to_string();
        if l.chars().count() > 80 {
            format!("{}…", l.chars().take(80).collect::<String>()).into()
        } else {
            SharedString::from(l)
        }
    })
}

/// Extract up to `max_lines` raw lines from the terminal (unfiltered),
/// taking the most recent lines from the visible content.
fn extract_raw_output(terminal: &Terminal, max_lines: usize) -> Vec<SharedString> {
    let raw_lines = terminal.last_n_non_empty_lines(50);

    // Filter out common bottom TUI chrome/status bar lines to capture the actual agent response.
    let filtered_lines: Vec<String> = raw_lines
        .into_iter()
        .filter(|l| is_meaningful_line(l))
        .collect();

    // Take the last `max_lines` of the response content, ordered chronologically.
    let mut result: Vec<SharedString> = filtered_lines
        .into_iter()
        .rev()
        .take(max_lines)
        .map(|l| {
            let l = l.trim().to_string();
            if l.chars().count() > 80 {
                format!("{}…", l.chars().take(80).collect::<String>()).into()
            } else {
                SharedString::from(l)
            }
        })
        .collect();
    result.reverse();
    result
}
