use gpui::{prelude::*, ScrollHandle, Window};
use std::path::PathBuf;
use ui::{Button, ButtonStyle, ButtonSize, Switch, ToggleState, Tooltip, prelude::*};
use util::ResultExt;

use crate::SettingsWindow;

struct SessionStats {
    checkpoint_count: usize,
    log_file_count: usize,
}

fn get_session_stats() -> SessionStats {
    let checkpoint_count = nir_analytics::get_state_db_path()
        .ok()
        .and_then(|path| nir_analytics::recent_checkpoints(&path).ok())
        .map(|records| records.len())
        .unwrap_or(0);
    let log_file_count = nir_analytics::list_log_files()
        .map(|files| files.len())
        .unwrap_or(0);
    SessionStats {
        checkpoint_count,
        log_file_count,
    }
}

fn logs_folder_path() -> Option<PathBuf> {
    nir_analytics::get_logs_dir().ok()
}

fn all_logs_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = logs_folder_path() {
        dirs.push(path);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok();
    if let Some(base) = base {
        let legacy = PathBuf::from(base).join(".nir/brain/logs");
        if !dirs.iter().any(|d| d == &legacy) {
            dirs.push(legacy);
        }
    }
    dirs
}

fn memory_folder_path() -> Option<PathBuf> {
    nir_analytics::get_memory_dir().ok()
}

const MEMORY_FILES: &[(&str, &str)] = &[
    ("about.md", "User identity and interests"),
    ("settings.md", "User preferences and rules"),
    ("goals.md", "Active goals and milestones"),
    ("projects.md", "Project paths and status"),
    ("bookmark.md", "Saved ideas and resources"),
];

fn reset_session_history() {
    if let Ok(state_db) = nir_analytics::get_state_db_path() {
        if let Err(err) = std::fs::remove_file(&state_db) {
            if err.kind() != std::io::ErrorKind::NotFound {
                log::error!("failed to remove {}: {err:#}", state_db.display());
            }
        }
    }

    if let Some(logs_dir) = logs_folder_path() {
        if logs_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&logs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                        if let Err(err) = std::fs::remove_file(&path) {
                            log::error!("failed to remove {}: {err:#}", path.display());
                        }
                    }
                }
            }
        }
    }

    log::info!("Session history reset");
}

pub(crate) fn render_agent_logs_page(
    settings_window: &SettingsWindow,
    _scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let session_stats = get_session_stats();
    let session_config = nir_analytics::load_session_config();
    let memory_dir = memory_folder_path();
    let original_window = settings_window.original_window;

    v_flex()
        .id("agent-memory-and-logs-page")
        .size_full()
        .pt_2()
        .pb_16()
        // === Session Recording Section ===
        .child(
            v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            Label::new("Session Recording")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            Switch::new(
                                "session-recording-toggle",
                                if session_config.enabled {
                                    ToggleState::Selected
                                } else {
                                    ToggleState::Unselected
                                },
                            )
                            .tab_index(0_isize)
                            .on_click(cx.listener(|_view, state, _window, cx| {
                                let new_config = nir_analytics::SessionConfig {
                                    enabled: *state == ToggleState::Selected,
                                };
                                if let Err(err) =
                                    nir_analytics::save_session_config(&new_config)
                                {
                                    log::error!(
                                        "Failed to save session config: {err:#}"
                                    );
                                }
                                cx.notify();
                            })),
                        ),
                )
                .child(
                    Label::new("Enables automatic recording of recall entries and session logs.")
                        .size(LabelSize::XSmall)
                        .color(Color::Disabled),
                )
                .child(Label::new(format!(
                    "Recall entries: {} (recent)",
                    session_stats.checkpoint_count
                )))
                .child(Label::new(format!(
                    "Log files: {}",
                    session_stats.log_file_count
                )))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("open-logs-folder", "Open logs folder")
                                .style(ButtonStyle::Outlined)
                                .size(ButtonSize::Compact)
                                .on_click(|_, _, cx| {
                                    for path in all_logs_dirs() {
                                        let _ = std::fs::create_dir_all(&path);
                                        cx.open_with_system(&path);
                                    }
                                }),
                        )
                        .child(
                            Button::new("reset-history", "Reset history")
                                .style(ButtonStyle::OutlinedGhost)
                                .size(ButtonSize::Compact)
                                .tooltip(Tooltip::text(
                                    "Remove ~/.nir/brain/state.db and all *.md log files in ~/.nir/brain/logs/.",
                                ))
                                .on_click(cx.listener(|_, _, _, cx| {
                                    reset_session_history();
                                    cx.notify();
                                })),
                        ),
                ),
        )
        // === Memory Files Section ===
        .child(
            v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            Label::new("Memory Files")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    Label::new("Persistent working memory the agent reads and writes automatically. Open to inspect or edit manually.")
                        .size(LabelSize::XSmall)
                        .color(Color::Disabled),
                )
                .children(MEMORY_FILES.iter().map(|(filename, description)| {
                    let file_path = memory_dir.as_ref().map(|dir| dir.join(filename));
                    let exists = file_path.as_ref().map_or(false, |p| p.exists());

                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(IconName::File).size(IconSize::Small).color(Color::Muted))
                        .child(
                            v_flex()
                                .child(Label::new(*filename).size(LabelSize::Small))
                                .child(
                                    Label::new(*description)
                                        .size(LabelSize::XSmall)
                                        .color(Color::Disabled),
                                ),
                        )
                        .child(
                            Button::new(
                                SharedString::from(*filename),
                                if exists { "Open" } else { "Create & Open" },
                            )
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener({
                                let file_path = file_path.clone();
                                move |_, _event, _window, cx| {
                                    let Some(path) = file_path.clone() else { return };
                                    if let Some(parent) = path.parent() {
                                        if let Err(err) = std::fs::create_dir_all(parent) {
                                            log::warn!(
                                                "failed to create directory {}: {err:#}",
                                                parent.display()
                                            );
                                            return;
                                        }
                                    }
                                    if !path.exists() {
                                        if let Err(err) = std::fs::write(&path, "") {
                                            log::warn!(
                                                "failed to create file {}: {err:#}",
                                                path.display()
                                            );
                                            return;
                                        }
                                    }
                                    if let Some(original_window) = original_window {
                                        original_window
                                            .update(cx, |multi_workspace, win, cx| {
                                                let workspace = multi_workspace.workspace().clone();
                                                workspace.update(cx, |workspace, cx| {
                                                    workspace
                                                        .open_abs_path(path, Default::default(), win, cx)
                                                        .detach_and_log_err(cx);
                                                });
                                            })
                                            .log_err();
                                    }
                                }
                            })),
                        )
                        .into_any_element()
                }))
                .child(
                    Button::new("open-memory-folder", "Open Memory Folder")
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .on_click(|_, _, cx| {
                            if let Some(path) = memory_folder_path() {
                                let _ = std::fs::create_dir_all(&path);
                                cx.open_with_system(&path);
                            }
                        }),
                ),
        )
        .into_any_element()
}
