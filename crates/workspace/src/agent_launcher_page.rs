use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, WeakEntity,
    Window, SharedString, StatefulInteractiveElement,
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
use std::time::{Duration, SystemTime};

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
        };

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
