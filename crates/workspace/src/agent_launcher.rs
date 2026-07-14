use crate::{
    SplitDirection, StatusItemView, Workspace,
    agent_launcher_page::AgentLauncherPage,
    workspace_settings::StatusBarSettings,
};
use gpui::{App, Context, Entity, Window, Empty};
use settings::{Settings, update_settings_file};
use ui::{ContextMenu, Tooltip, prelude::*, right_click_menu};

pub struct AgentLauncherButton {
    workspace: Entity<Workspace>,
}

impl AgentLauncherButton {
    pub fn new(workspace: Entity<Workspace>) -> Self {
        Self { workspace }
    }
}

impl Render for AgentLauncherButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !StatusBarSettings::get_global(cx).show_agent_launcher {
            return Empty.into_any_element();
        }

        let workspace = self.workspace.clone();

        right_click_menu("agent-launcher-menu")
            .trigger(move |_is_active, _window, _cx| {
                Button::new("agent-launcher", "Terminal Agent")
                    .label_size(LabelSize::Small)
                    .style(ButtonStyle::Subtle)
                    .color(Color::Muted)
                    .start_icon(Icon::new(IconName::Terminal).color(Color::Muted))
                    .tooltip(Tooltip::text("Launch Terminal AI Agents"))
                    .on_click(move |_clicked, window, cx| {
                        workspace.update(cx, |workspace, cx| {
                            let existing = workspace
                                .items_of_type::<AgentLauncherPage>(cx)
                                .next();
                            if let Some(existing) = existing {
                                workspace.activate_item(&existing, true, true, window, cx);
                                return;
                            }

                            let item = cx.new(|cx| {
                                AgentLauncherPage::new(workspace.weak_handle(), cx)
                            });

                            let active_pane_has_files = workspace
                                .active_pane()
                                .read(cx)
                                .items()
                                .any(|i| i.project_path(cx).is_some());

                            if active_pane_has_files {
                                workspace.split_item(
                                    SplitDirection::Right,
                                    Box::new(item),
                                    window,
                                    cx,
                                );
                            } else {
                                workspace.add_item_to_active_pane(
                                    Box::new(item),
                                    None,
                                    true,
                                    window,
                                    cx,
                                );
                            }
                        });
                    })
                    .into_any_element()
            })
            .menu(move |window, cx| {
                ContextMenu::build(window, cx, |menu, _window, _cx| {
                    menu.entry("Hide Button", None, |_window, cx| {
                        let fs = <dyn fs::Fs>::global(cx);
                        update_settings_file(fs, cx, |settings, _cx| {
                            settings
                                .status_bar
                                .get_or_insert_default()
                                .show_agent_launcher = Some(false);
                        });
                    })
                })
            })
            .into_any_element()
    }
}

impl StatusItemView for AgentLauncherButton {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn crate::ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn hide_setting(&self, _cx: &App) -> Option<crate::status_bar::HideStatusItem> {
        Some(crate::status_bar::HideStatusItem::new(|settings| {
            settings.status_bar.get_or_insert_default().show_agent_launcher = Some(false);
        }))
    }
}
