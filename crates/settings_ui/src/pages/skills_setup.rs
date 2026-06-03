use agent::analytics::{approve_discovered_skill, reject_discovered_skill, AnalyticsConfig, SkillIndexEntry, SkillsIndex, get_manual_analysis_stats};
use agent_skills::{Skill, SkillIndex, encode_skill_share_link};
use gpui::{Action as _, ClipboardItem, ScrollHandle, SharedString, prelude::*};
use language_model::LanguageModelRegistry;
use std::fs;

use ui::{Divider, IconButtonShape, TintColor, Tooltip, prelude::*};
use util::ResultExt as _;

use crate::{SettingsUiFile, SettingsWindow};

pub(crate) fn render_skills_setup_page(
    settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let skill_index = cx.try_global::<SkillIndex>();

    let skills: Vec<Skill> = match &settings_window.current_file {
        SettingsUiFile::User => skill_index
            .map(|idx| idx.global_skills.clone())
            .unwrap_or_default(),
        SettingsUiFile::Project((worktree_id, _)) => {
            let worktree_id = usize::from(*worktree_id);
            skill_index
                .and_then(|index| {
                    index
                        .project_skills
                        .iter()
                        .find(|group| group.worktree_id.0 == worktree_id)
                        .map(|group| group.skills.clone())
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
    .into_iter()
    .filter(|skill| {
        !settings_window
            .hidden_deleted_skill_directory_paths
            .contains(&skill.directory_path)
    })
    .collect();

    let discovered_skills = if matches!(settings_window.current_file, SettingsUiFile::User) {
        load_discovered_skills()
    } else {
        Vec::new()
    };

    v_flex()
        .id("skills-page")
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .map(|this| {
            if skills.is_empty() && discovered_skills.is_empty() {
                let message = match &settings_window.current_file {
                    SettingsUiFile::User => "No global skills installed.",
                    SettingsUiFile::Project(_) => "No project skills found.",
                    _ => "No skills available for this context.",
                };
                let original_window = settings_window.original_window;
                this.items_center().justify_center().child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .child(Label::new(message).color(Color::Muted))
                        .child(
                            Button::new("open-skill-creator", "Create a Skill")
                                .tab_index(0_isize)
                                .style(ButtonStyle::Outlined)
                                .end_icon(
                                    Icon::new(IconName::ArrowUpRight)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .on_click(cx.listener(move |_this, _event, window, cx| {
                                    let Some(original_window) = original_window else {
                                        return;
                                    };
                                    original_window
                                        .update(cx, |_workspace, original_window, cx| {
                                            original_window.dispatch_action(
                                                zed_actions::assistant::OpenSkillCreator
                                                    .boxed_clone(),
                                                cx,
                                            );
                                        })
                                        .log_err();
                                    window.remove_window();
                                })),
                        ),
                )
            } else {
                let mut elements: Vec<AnyElement> = Vec::new();

                if matches!(settings_window.current_file, SettingsUiFile::User) {
                    let analytics_config = AnalyticsConfig::load();
                    let is_enabled = analytics_config.enabled;

                    // Always load fresh stats on render
                    let stats = get_manual_analysis_stats();

                    // Analytics engine section
                    let analytics_section = v_flex()
                        .gap_3()
                        .p_3()
                        .rounded_md()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    h_flex()
                                        .gap_3()
                                        .items_center()
                                        .child(
                                            Label::new("Skill Discovery")
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                        .child(
                                            IconButton::new(
                                                "skill-discovery-info",
                                                IconName::Info,
                                            )
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .shape(IconButtonShape::Square)
                                            .style(ButtonStyle::Transparent)
                                            .tooltip(Tooltip::text(
                                                "Learns patterns from your task history and generates reusable skills. Toggle on/off, or refresh to check for changes.",
                                            ))
                                            .on_click(|_, _, _| {}),
                                        )
                                        .child(
                                            Button::new(
                                                "analytics-toggle",
                                                if is_enabled { "On" } else { "Off" },
                                            )
                                            .style(if is_enabled {
                                                ButtonStyle::Tinted(TintColor::Success)
                                            } else {
                                                ButtonStyle::Outlined
                                            })
                                            .size(ButtonSize::Compact)
                                            .tooltip(Tooltip::text(if is_enabled {
                                                "Click to disable automatic skill discovery"
                                            } else {
                                                "Click to enable automatic skill discovery"
                                            }))
                                            .on_click(cx.listener(move |_, _, _, cx| {
                                                let mut config = AnalyticsConfig::load();
                                                config.enabled = !config.enabled;
                                                let _ = config.save();
                                                cx.notify();
                                            })),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .gap_3()
                                        .items_center()
                                        .child(
                                            Button::new("refresh-stats", "Refresh")
                                                .style(ButtonStyle::Outlined)
                                                .size(ButtonSize::Compact)
                                                .tooltip(Tooltip::text("Re-read logs and registry to update stats"))
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("analyze-now", "Analyze Now")
                                                .style(ButtonStyle::Outlined)
                                                .size(ButtonSize::Compact)
                                                .tooltip(Tooltip::text("Run full analysis — parses logs, clusters patterns, promotes eligible skills"))
                                                .on_click(cx.listener(move |_, _, _window, cx| {
                                                    cx.spawn(async move |this, cx| {
                                                        let model = cx.update(|cx| {
                                                            LanguageModelRegistry::read_global(cx).default_model().map(|m| m.model)
                                                        });

                                                        let Some(model) = model else {
                                                            log::warn!("Skill Discovery: No model available for manual analysis");
                                                            return;
                                                        };

                                                        log::info!("Skill Discovery: Manual analysis starting with model: {}", model.name().0);
                                                        let result = agent::analytics::run_analytics_cycle(model, &cx).await;
                                                        match result {
                                                            Ok(skills) => {
                                                                log::info!("Skill Discovery: Manual analysis complete — {} skill(s) promoted", skills.len());
                                                                this.update(cx, |_, cx| cx.notify()).ok();
                                                            }
                                                            Err(err) => {
                                                                log::error!("Skill Discovery: Manual analysis failed: {:?}", err);
                                                            }
                                                        }
                                                    }).detach();
                                                })),
                                        )
                                        .child(
                                            Button::new("reset-analytics", "Reset")
                                                .style(ButtonStyle::OutlinedGhost)
                                                .size(ButtonSize::Compact)
                                                .tooltip(Tooltip::text("Clear all staged patterns and disable discovery. Logs and approved skills are preserved."))
                                                .on_click(cx.listener(move |_settings_window, _, _window, cx| {
                                                    if let Err(err) = agent::analytics::cleanup_analytics() {
                                                        log::error!("Skill Discovery cleanup failed: {:?}", err);
                                                    }
                                                    log::info!("Skill Discovery: Cleaned up state");
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_4()
                                .child(Label::new(format!("Logs: {}", stats.total_logs)))
                                .child(Label::new(format!("Checkpoints: {}", stats.parsed_checkpoints)))
                                .child(Label::new(format!("Staged: {}", stats.staged_recollections)))
                                .child(Label::new(format!("Eligible: {}", stats.eligible_for_promotion)))
                                .child(Label::new(format!("Discovered: {}", stats.discovered_skills))),
                        );

                    elements.push(analytics_section.into_any_element());
                    elements.push(Divider::horizontal().into_any_element());

                    // Discovered skills section
                    let mut discovered_section = v_flex().gap_3().pt_4().child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Label::new("Discovered Skills (Pending Review)")
                                    .size(LabelSize::Small),
                            ),
                    );

                    if discovered_skills.is_empty() {
                        discovered_section = discovered_section.child(
                            div().pb_2().child(
                                Label::new("None.")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                        );
                    } else {
                        discovered_section = discovered_section.children(
                            discovered_skills
                                .iter()
                                .map(|skill| render_discovered_skill_row(skill, cx)),
                        );
                    }

                    elements.push(discovered_section.into_any_element());

                    if !skills.is_empty() {
                        elements.push(Divider::horizontal().into_any_element());
                    }
                }

                elements.extend(
                    skills
                        .iter()
                        .enumerate()
                        .flat_map(|(i, skill)| {
                            let mut rows: Vec<AnyElement> =
                                vec![render_skill_row(skill, settings_window, cx)];
                            if i + 1 < skills.len() {
                                rows.push(Divider::horizontal().into_any_element());
                            }
                            rows
                        }),
                );

                this.track_scroll(scroll_handle)
                    .overflow_y_scroll()
                    .children(elements)
            }
        })
        .into_any_element()
}

fn load_discovered_skills() -> Vec<SkillIndexEntry> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();

    let Some(home_path) = home.map(std::path::PathBuf::from) else {
        return Vec::new();
    };

    let index_path = home_path.join(".nir/brain/skills_index.json");
    if !index_path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(&index_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let index: SkillsIndex = match serde_json::from_str(&content) {
        Ok(index) => index,
        Err(_) => return Vec::new(),
    };

    index.discovered_skills
}

fn render_skill_row(
    skill: &Skill,
    settings_window: &SettingsWindow,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let skill_file_path = skill.skill_file_path.clone();
    let directory_path = skill.directory_path.clone();

    let share_copied = settings_window.last_copied_skill_directory_path.as_deref()
        == Some(skill.directory_path.as_path());
    let (share_icon, share_icon_color) = if share_copied {
        (IconName::Check, Color::Success)
    } else {
        (IconName::Link, Color::Muted)
    };

    h_flex()
        .w_full()
        .justify_between()
        .py_2p5()
        .gap_4()
        .child(
            v_flex()
                .gap_0p5()
                .min_w_0()
                .flex_1()
                .child(Label::new(skill.name.clone()))
                .child(
                    Label::new(skill.description.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(
            h_flex()
                .gap_2()
                .child({
                    let share_skill_file_path = skill.skill_file_path.clone();
                    let share_directory_path = skill.directory_path.clone();
                    IconButton::new(
                        SharedString::from(format!("share-{}", skill.name)),
                        share_icon,
                    )
                    .tab_index(0_isize)
                    .icon_size(IconSize::Small)
                    .icon_color(share_icon_color)
                    .tooltip(Tooltip::text("Copy Share Link"))
                    .on_click(cx.listener(
                        move |_settings_window, _event, _window, cx| {
                            let skill_file_path = share_skill_file_path.clone();
                            let directory_path = share_directory_path.clone();
                            cx.spawn(async move |settings_window, cx| {
                                match std::fs::read_to_string(&skill_file_path) {
                                    Ok(content) => {
                                        let link = encode_skill_share_link(&content);
                                        settings_window
                                            .update(cx, |settings_window, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    link,
                                                ));
                                                settings_window.last_copied_skill_directory_path =
                                                    Some(directory_path.clone());
                                                cx.notify();
                                            })
                                            .ok();
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "failed to read skill file {} for sharing: {error:#}",
                                            skill_file_path.display()
                                        );
                                    }
                                }
                            })
                            .detach();
                        },
                    ))
                })
                .child(
                    IconButton::new(
                        SharedString::from(format!("delete-{}", skill.name)),
                        IconName::Trash,
                    )
                    .tab_index(0_isize)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Delete Skill"))
                    .on_click(cx.listener(
                        move |settings_window, _event, _window, cx| {
                            let directory_path = directory_path.clone();
                            if !settings_window
                                .hidden_deleted_skill_directory_paths
                                .insert(directory_path.clone())
                            {
                                return;
                            }
                            cx.notify();

                            cx.spawn(async move |settings_window, cx| {
                                let remove_result = if directory_path.exists() {
                                    fs::remove_dir_all(&directory_path)
                                } else {
                                    Ok(())
                                };
                                if let Err(error) = remove_result {
                                    log::error!(
                                        "failed to delete skill directory {}: {error:#}",
                                        directory_path.display()
                                    );
                                    settings_window
                                        .update(cx, |settings_window, cx| {
                                            settings_window
                                                .hidden_deleted_skill_directory_paths
                                                .remove(&directory_path);
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            })
                            .detach();
                        },
                    )),
                )
                .child(
                    Button::new(SharedString::from(format!("open-{}", skill.name)), "Open")
                        .tab_index(0_isize)
                        .style(ButtonStyle::OutlinedGhost)
                        .size(ButtonSize::Medium)
                        .end_icon(
                            Icon::new(IconName::ArrowUpRight)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .on_click(cx.listener(move |settings_window, _event, window, cx| {
                            let skill_file_path = skill_file_path.clone();
                            let Some(original_window) = settings_window.original_window else {
                                return;
                            };
                            original_window
                                .update(cx, |multi_workspace, original_window, cx| {
                                    let workspace = multi_workspace.workspace().clone();
                                    workspace.update(cx, |workspace, cx| {
                                        workspace
                                            .open_abs_path(
                                                skill_file_path,
                                                Default::default(),
                                                original_window,
                                                cx,
                                            )
                                            .detach_and_log_err(cx);
                                    });
                                })
                                .log_err();
                            window.remove_window();
                        })),
                ),
        )
        .into_any_element()
}

fn render_discovered_skill_row(
    skill: &SkillIndexEntry,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let slug = skill.name.clone();
    let slug_reject = skill.name.clone();
    let colors = cx.theme().colors();

    v_flex()
        .w_full()
        .p_2p5()
        .gap_2()
        .bg(colors.surface_background.opacity(0.15))
        .border_1()
        .border_color(colors.border)
        .rounded_sm()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .gap_4()
                .child(
                    v_flex()
                        .gap_0p5()
                        .min_w_0()
                        .flex_1()
                        .child(Label::new(skill.name.clone()))
                        .child(
                            Label::new(skill.description.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new(
                                SharedString::from(format!("approve-{}", slug)),
                                "Approve & Enable",
                            )
                            .tab_index(0_isize)
                            .style(ButtonStyle::Tinted(TintColor::Success))
                            .size(ButtonSize::Medium)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                if let Err(err) = approve_discovered_skill(&slug) {
                                    log::error!("approve_discovered_skill({}) failed: {err:#}", slug);
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            Button::new(
                                SharedString::from(format!("reject-{}", slug_reject)),
                                "Reject",
                            )
                            .tab_index(0_isize)
                            .style(ButtonStyle::Tinted(TintColor::Error))
                            .size(ButtonSize::Medium)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                if let Err(err) = reject_discovered_skill(&slug_reject) {
                                    log::error!("reject_discovered_skill({}) failed: {err:#}", slug_reject);
                                }
                                cx.notify();
                            })),
                        ),
                ),
        )
        .into_any_element()
}
