use gpui::{prelude::*, ScrollHandle, Window};
use ui::{Button, ButtonStyle, ButtonSize, Switch, ToggleState, Tooltip, prelude::*};

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

fn logs_folder_path() -> Option<std::path::PathBuf> {
    let home =
        std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()?;
    Some(std::path::PathBuf::from(home).join(".nir/brain/logs"))
}

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
    _settings_window: &SettingsWindow,
    _scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let session_stats = get_session_stats();
    let session_config = nir_analytics::load_session_config();

    v_flex()
        .id("session-history-page")
        .size_full()
        .pt_2()
        .pb_16()
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
                                    if let Some(path) = logs_folder_path() {
                                        if let Err(err) = std::fs::create_dir_all(&path) {
                                            log::warn!(
                                                "failed to ensure logs folder {}: {err:#}",
                                                path.display()
                                            );
                                            return;
                                        }
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
        .into_any_element()
}
