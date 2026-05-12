use gpui::{
    Animation, AnimationExt, App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Render, Subscription, WeakEntity, Window, SharedString, StatefulInteractiveElement,
    pulsating_between,
};
use terminal::{
    Terminal,
    alacritty_terminal::term::cell::Flags,
};
use ui::prelude::*;
use ui::Tooltip;
use crate::{
    OpenOptions, Workspace,
    agent_config::{AgentConfig, config_path, load_config},
    item::{Item, ItemEvent},
    pane::Pane,
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

        // Fast live streaming poller: poll running agents output every 300ms for real-time mini preview updates
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                    let ok = this
                        .update(&mut cx, |this, cx| {
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

        // Prune entries whose terminals have been dropped.
        self.running_agents.retain(|agent| {
            agent
                .terminal
                .upgrade()
                .map(|t| t.read_with(cx, |t, _| t.task().is_some()))
                .unwrap_or(false)
        });

        let Some(workspace) = self.workspace.upgrade() else { return };
        let panes = workspace.read(cx).panes().to_vec();

        // Collect all agent terminals from all panes.
        // Skip our own AgentLauncherPage item to avoid read-during-update panic.
        let self_id = cx.entity().entity_id();
        for pane in &panes {
            let items: Vec<_> = pane.read(cx).items().cloned().collect();

            for item in items {
                // Skip items that are our own tab (same entity).
                if item.item_id() == self_id {
                    continue;
                }
                let Some(terminal) = item.act_as_terminal(cx) else { continue };

                let is_agent = terminal
                    .read_with(cx, |t, _| {
                        t.task()
                            .map(|task| task.spawned_task.id.0.starts_with("agent-"))
                            .unwrap_or(false)
                    });
                if !is_agent {
                    continue;
                }

                // Get agent name from task info.
                let name: SharedString = terminal
                    .read_with(cx, |t, _| {
                        t.task()
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
                            .unwrap_or_default()
                    })
                    .into();

                // Already tracked?
                if self.running_agents.iter().any(|a| a.name == name) {
                    continue;
                }

                // Subscribe to Wakeup events.
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

                let agent_name = name.clone();
                self.running_agents.push(RunningAgent {
                    name: agent_name,
                    terminal: terminal.downgrade(),
                    output_lines: Vec::new(),
                    filtered_line: None,
                    is_active: true,
                    is_expanded: true,
                    last_update: Instant::now(),
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
        let (name, filtered, raw_lines, is_active) = terminal.read_with(cx, |t, _| {
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
            let is_active =
                t.task().is_some_and(|task| task.status == terminal::TaskStatus::Running);
            (name, filtered, raw_lines, is_active)
        });

        if let Some(agent) = self
            .running_agents
            .iter_mut()
            .find(|a| a.name == name)
        {
            agent.output_lines = raw_lines;
            agent.filtered_line = filtered;
            agent.is_active = is_active;
            agent.last_update = Instant::now();
        }
        cx.notify();
    }

    /// Poll all tracked running agents to stream live terminal responses.
    fn poll_running_agents(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for agent in &mut self.running_agents {
            if let Some(term) = agent.terminal.upgrade() {
                let (filtered, raw_lines, is_active) = term.read_with(cx, |t, _| {
                    let filtered = extract_meaningful_output(t);
                    let raw_lines = extract_raw_output(t, 8);
                    let is_active = t.task().is_some_and(|task| task.status == terminal::TaskStatus::Running);
                    (filtered, raw_lines, is_active)
                });
                if agent.output_lines != raw_lines || agent.is_active != is_active {
                    agent.output_lines = raw_lines;
                    agent.filtered_line = filtered;
                    agent.is_active = is_active;
                    agent.last_update = Instant::now();
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Render the running agents preview card at the top of the launcher.
    fn render_running_agents(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        if self.running_agents.is_empty() {
            return None;
        }

        // Sort by most recent update, take top 3.
        let mut sorted: Vec<&RunningAgent> = self.running_agents.iter().collect();
        sorted.sort_by(|a, b| b.last_update.cmp(&a.last_update));
        let top: Vec<&&RunningAgent> = sorted.iter().take(3).collect();

        let border_color = cx.theme().colors().border;

        Some(
            // ── Centered wrapper with max width ──────────────────────────
            h_flex()
                .w_full()
                .justify_center()
                .px_5()
                .py_2()
                .child(
                    v_flex()
                        .w_full()
                        .max_w(rems(48.))
                        .gap_2()
                        // ── Collapsible header ────────────────────────────
                        .child(
                            div()
                                .id("running-agents-header")
                                .cursor_pointer()
                                .rounded_md()
                                .hover(|s| s.bg(cx.theme().colors().element_hover))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preview_expanded = !this.preview_expanded;
                                    cx.notify();
                                }))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_2()
                                        .py_1()
                                        .px_2()
                                        .child(
                                            Icon::new(if self.preview_expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            })
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                        )
                                        .child(
                                            Icon::new(IconName::Terminal)
                                                .size(IconSize::Small)
                                                .color(Color::Info),
                                        )
                                        .child(
                                            Label::new("Running Agents")
                                                .size(LabelSize::Small)
                                                .weight(gpui::FontWeight::SEMIBOLD),
                                        )
                                        .child(
                                            Label::new(format!("({})", sorted.len()))
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted),
                                        )
                                        .child(div().flex_1())
                                        .child(
                                            Label::new(if self.preview_expanded {
                                                "collapse"
                                            } else {
                                                "expand"
                                            })
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                        ),
                                ),
                        )
                        // ── Expanded agent rows ──────────────────────────
                        .when(self.preview_expanded, |this| {
                            this.children(top.into_iter().map(|agent| {
                                Self::render_agent_row(agent, cx)
                            }))
                        }),
                ),
        )
    }

    /// Render a single running agent row with pulse dot, output lines, and actions.
    fn render_agent_row(agent: &RunningAgent, cx: &Context<Self>) -> impl IntoElement {
        // ── Pulsing dot animation (active only) ──────────────────────────
        let dot: AnyElement = if agent.is_active {
            let pulse = Animation::new(Duration::from_secs(2))
                .repeat()
                .with_easing(pulsating_between(0.3, 1.0));
            div()
                .child(
                    Icon::new(IconName::ArrowCircle)
                        .size(IconSize::XSmall)
                        .color(Color::Info),
                )
                .with_animation("agent-pulse", pulse, |el, delta| el.opacity(delta))
                .into_any_element()
        } else {
            div().child(
                Icon::new(IconName::Check)
                    .size(IconSize::XSmall)
                    .color(Color::Created),
            )
            .into_any_element()
        };

        // ── Output lines ────────────────────────────────────────────────
        let output_lines = if agent.output_lines.is_empty() {
            // Fall back to filtered line if no raw lines yet.
            if let Some(ref filtered) = agent.filtered_line {
                vec![filtered.clone()]
            } else {
                vec![SharedString::from("waiting for output...")]
            }
        } else {
            agent.output_lines.clone()
        };

        let name = agent.name.clone();
        let is_expanded = agent.is_expanded;

        v_flex()
            .w_full()
            .gap_2()
            .py_2()
            .px_3()
            .rounded_lg()
            .bg(cx.theme().colors().elevated_surface_background.opacity(0.8))
            .border_1()
            .border_color(cx.theme().colors().border.opacity(0.5))
            .child(
                // ── Agent header: collapse chevron + dot + name + go-to button ──────────────
                div()
                    .id(format!("agent-hdr-row-{}", agent.name))
                    .flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
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
                        Icon::new(if is_expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall)
                        .color(if is_expanded { Color::Accent } else { Color::Muted }),
                    )
                    .child(dot)
                    .child(
                        Label::new(agent.name.clone())
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(div().flex_1())
                    .child(
                        IconButton::new(
                            format!("goto-agent-{}", agent.name),
                            IconName::OpenNewWindow,
                        )
                        .icon_size(IconSize::XSmall)
                        .size(ButtonSize::Compact)
                        .icon_color(Color::Muted)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.jump_to_agent_terminal(&name, window, cx);
                        }))
                        .tooltip(Tooltip::text("Go to agent terminal")),
                    ),
            )
            // ── Output lines ──────────────────────────────────────────────
            .when(is_expanded, |this| {
                this.children(output_lines.iter().map(|line| {
                    div()
                        .pl(DynamicSpacing::Base06.rems(cx))
                        .child(
                            Label::new(line.clone())
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                }))
            })
    }

    /// Jump to the terminal tab for a given agent name.
    fn jump_to_agent_terminal(
        &mut self,
        agent_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let self_id = cx.entity().entity_id();

        if let Some(workspace) = self.workspace.upgrade() {
            // Collect panes upfront to release the cx borrow before update.
            let panes: Vec<Entity<Pane>> = workspace.read(cx).panes().to_vec();

            for pane in &panes {
                let pane_items: Vec<_> = pane.read(cx).items().cloned().collect();
                for (idx, item) in pane_items.iter().enumerate() {
                    if item.item_id() == self_id {
                        continue;
                    }
                    let Some(terminal) = item.act_as_terminal(cx) else {
                        continue;
                    };
                    let is_match = terminal.read_with(cx, |t, _| {
                        t.task().is_some_and(|task| {
                            let name = if !task.spawned_task.full_label.is_empty() {
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
                            };
                            name == agent_name
                        })
                    });
                    if is_match {
                        pane.update(cx, |pane, cx| {
                            pane.activate_item(idx, true, true, window, cx);
                        });
                        return;
                    }
                }
            }
        }
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

    matches!(
        std::process::Command::new(cmd).arg(binary).output(),
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
        // Lazily scan for agent terminals on first render (avoids lease conflicts
        // during construction, since render runs after all updates complete).
        self.update_running_agents(cx);

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
                                                Label::new("AI Command Center for Void")
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
            // ── Scrollable agent list ─────────────────────────────────
            .child(
                div()
                    .id("launcher-scroll-container")
                    .flex_1()
                    .overflow_y_scroll()
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
                                        .bg(surface.opacity(0.2))
                                        .border_1()
                                        .border_color(if is_expanded {
                                            accent.opacity(0.4)
                                        } else {
                                            border_color.opacity(0.4)
                                        })
                                        .rounded_md()
                                        // Collapsed row
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
                                                        .gap_2()
                                                        .items_center()
                                                        .child(
                                                            Icon::new(if is_expanded {
                                                                IconName::ChevronDown
                                                            } else {
                                                                IconName::ChevronRight
                                                            })
                                                            .size(IconSize::XSmall)
                                                            .color(if is_expanded {
                                                                Color::Accent
                                                            } else {
                                                                Color::Muted
                                                            }),
                                                        )
                                                        .child(
                                                            Label::new(name.clone())
                                                                .weight(gpui::FontWeight::SEMIBOLD)
                                                                .size(LabelSize::Small),
                                                        )
                                                        .child(
                                                            div()
                                                                .w(px(6.))
                                                                .h(px(6.))
                                                                .rounded_full()
                                                                .bg(if is_installed {
                                                                    success
                                                                } else if is_not_installed {
                                                                    accent.opacity(0.6)
                                                                } else {
                                                                    cx.theme()
                                                                        .colors()
                                                                        .text_muted
                                                                        .opacity(0.3)
                                                                }),
                                                        ),
                                                )
                                                .child(
                                                    h_flex()
                                                        .gap_2()
                                                        .items_center()
                                                        .when(is_not_installed, |this| {
                                                            this.child(
                                                                Label::new("not installed")
                                                                    .size(LabelSize::XSmall)
                                                                    .color(Color::Accent),
                                                            )
                                                        })
                                                        .when(is_installed, |this| {
                                                            this.child(
                                                                Button::new(
                                                                    format!("launch-{i}"),
                                                                    "LAUNCH",
                                                                )
                                                                .style(ButtonStyle::Filled)
                                                                .size(ButtonSize::Compact)
                                                                .on_click(cx.listener(
                                                                    move |this, _, window, cx| {
                                                                        this.launch_agent(
                                                                            i, window, cx,
                                                                        );
                                                                    },
                                                                )),
                                                            )
                                                        }),
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
const NOISE_PREFIXES: &[&str] = &[
    "$ ", "% ", "> ", "# ", "❯ ", "→ ", "λ ",
    "[", "│", "┌", "├", "└", "─", "━", "╭", "╰", "╰",
];

/// Extract the last meaningful line of output from a terminal, filtering out
/// TUI noise, shell prompts, and progress indicators — preferring code/text.
fn extract_meaningful_output(terminal: &Terminal) -> Option<SharedString> {
    let content = terminal.last_content();
    if content.cells.is_empty() {
        return None;
    }

    // Group cells into lines.
    let mut raw_lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut prev_line: Option<usize> = None;

    for ic in &content.cells {
        if ic.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let line_num = ic.point.line.0 as usize;
        match prev_line {
            None => {
                current_line.push(ic.c);
                prev_line = Some(line_num);
            }
            Some(p) if p == line_num => {
                current_line.push(ic.c);
            }
            Some(_) => {
                let trimmed = current_line.trim_end().to_string();
                if !trimmed.is_empty() {
                    raw_lines.push(trimmed);
                }
                current_line = String::new();
                current_line.push(ic.c);
                prev_line = Some(line_num);
            }
        }
    }
    // Flush last line.
    let trimmed = current_line.trim_end().to_string();
    if !trimmed.is_empty() {
        raw_lines.push(trimmed);
    }

    // Save a copy for the fallback before consuming raw_lines.
    let last_non_empty: Option<String> = raw_lines
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .cloned();

    // Filter: scan from bottom, return first "meaningful" line.
    for line in raw_lines.into_iter().rev() {
        let line = line.trim().to_string();
        if line.is_empty() || line.len() < 3 {
            continue;
        }
        // Skip if starts with a noise prefix.
        if NOISE_PREFIXES.iter().any(|p| line.starts_with(p)) {
            continue;
        }
        // Skip common TUI status messages.
        if line.contains("──")
            || line.contains("━━")
            || line.contains("Analyzing")
            || line.contains("Loading")
            || line.contains("Processing")
            || line.contains("Context")
            || line.starts_with("context")
            || line.contains("|/") 
            || line.contains("/-\\")
        {
            continue;
        }
        // Looks like meaningful output — prefer lines with code indicators.
        let has_code_chars = line.contains('{')
            || line.contains('}')
            || line.contains('(')
            || line.contains(')')
            || line.starts_with("  ")
            || line.starts_with('\t');
        if has_code_chars || line.len() > 20 {
            // Truncate long lines
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
    let content = terminal.last_content();
    if content.cells.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_line: Option<usize> = None;

    for ic in &content.cells {
        if ic.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let line_num = ic.point.line.0 as usize;
        match prev_line {
            None => {
                current.push(ic.c);
                prev_line = Some(line_num);
            }
            Some(p) if p == line_num => {
                current.push(ic.c);
            }
            Some(_) => {
                let trimmed = current.trim_end().to_string();
                if !trimmed.is_empty() {
                    lines.push(trimmed);
                }
                current = String::new();
                current.push(ic.c);
                prev_line = Some(line_num);
            }
        }
    }
    let trimmed = current.trim_end().to_string();
    if !trimmed.is_empty() {
        lines.push(trimmed);
    }

    // Filter out common bottom TUI chrome/status bar lines to capture the actual agent response.
    let filtered_lines: Vec<String> = lines
        .into_iter()
        .filter(|l| {
            let s = l.trim();
            if s.is_empty() {
                return false;
            }
            if s.starts_with("workspace ") || s.starts_with("~/") || s.starts_with(".\\") || s.starts_with("./") {
                return false;
            }
            if s.contains("sandbox") || s.contains("Shift+Tab") || s.starts_with("Shift+Tab") {
                return false;
            }
            if s.contains("───") || s.contains("━━━") || s.contains("════") {
                return false;
            }
            if s.starts_with("> Type your message") || s.starts_with("> █") {
                return false;
            }
            true
        })
        .collect();

    // Take the last `max_lines` of the response content, ordered chronologically.
    let mut result: Vec<SharedString> = filtered_lines
        .into_iter()
        .rev()
        .take(max_lines)
        .map(|l| {
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
