use gpui::{IntoElement, RenderOnce, Window, App};
use ui::{prelude::*, Color, Label, LabelSize, Vector, VectorName};

#[derive(IntoElement)]
pub struct EmptyState {
    pub has_worktrees: bool,
}

impl EmptyState {
    pub fn new(has_worktrees: bool) -> Self {
        Self { has_worktrees }
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let shortcuts: &[(&str, &str)] = if self.has_worktrees {
            &[
                ("Open File", "Ctrl+O"),
                ("Show All Commands", "Ctrl+Shift+P"),
                ("Toggle Terminal", "Ctrl+~"),
            ]
        } else {
            &[
                ("Open File", "Ctrl+O"),
                ("Open Project", "Ctrl+K Ctrl+O"),
                ("Open Recent", "Ctrl+R"),
            ]
        };

        v_flex()
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .opacity(0.1)
                    .child(Vector::square(VectorName::VoidLogo, rems_from_px(100.)))
            )
            .child(
                v_flex()
                    .mt_8()
                    .gap_2()
                    .w_64()
                    .children(shortcuts.iter().map(|(action, key)| {
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(
                                Label::new(*action)
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                            .child(
                                Label::new(*key)
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                    }))
            )
    }
}

