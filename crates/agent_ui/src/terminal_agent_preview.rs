use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use gpui::{App, Entity, SharedString, Subscription, WeakEntity};
use terminal::{
    Terminal,
    alacritty_terminal::term::cell::Flags,
};

use task::SpawnInTerminal;

/// Max preview lines shown in the strip.
pub const MAX_PREVIEW_LINES: usize = 4;
/// Max chars per line before truncating with `…`.
const MAX_LINE_CHARS: usize = 80;
/// How long after last output to keep the "done" state visible.
const DONE_FADE_DURATION: Duration = Duration::from_secs(3);
/// Debounce interval for re-rendering on rapid terminal output.
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(200);

/// Whether the terminal's spawned task was launched by the Agent Launcher.
pub fn is_agent_terminal(task: &SpawnInTerminal) -> bool {
    task.id.0.starts_with("agent-")
}

/// Extract the last N visible lines from terminal cells as a vector of strings.
///
/// Lines are ordered top-to-bottom (oldest first), each trimmed to `MAX_LINE_CHARS`.
pub fn extract_preview_lines(terminal: &Terminal) -> Vec<SharedString> {
    let content = terminal.last_content();
    if content.cells.is_empty() {
        return Vec::new();
    }

    // Group cells by line number; cells are in display order (top-left to bottom-right).
    let mut lines: Vec<(usize, String)> = Vec::new();
    let mut current_line: Option<usize> = None;
    let mut current_text = String::new();

    for ic in &content.cells {
        let line = ic.point.line.0 as usize;

        // Skip wide-char spacers to avoid duplicate/empty characters.
        if ic.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        match current_line {
            None => {
                current_line = Some(line);
                current_text.push(ic.c);
            }
            Some(prev_line) if line == prev_line => {
                current_text.push(ic.c);
            }
            Some(_) => {
                // Line changed — flush current.
                let trimmed = current_text.trim_end().to_string();
                if !trimmed.is_empty() {
                    let truncated = truncate_line(&trimmed);
                    lines.push((current_line.unwrap(), truncated));
                }
                current_text = String::new();
                current_text.push(ic.c);
                current_line = Some(line);
            }
        }
    }

    // Flush the last line.
    if current_line.is_some() {
        let trimmed = current_text.trim_end().to_string();
        if !trimmed.is_empty() {
            let truncated = truncate_line(&trimmed);
            lines.push((current_line.unwrap(), truncated));
        }
    }

    // Sort by line number and take the last MAX_PREVIEW_LINES lines.
    lines.sort_by_key(|(line, _)| *line);
    lines
        .into_iter()
        .rev()
        .take(MAX_PREVIEW_LINES)
        .map(|(_, text)| SharedString::from(text))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn truncate_line(s: &str) -> String {
    if s.chars().count() > MAX_LINE_CHARS {
        format!(
            "{}…",
            s.chars().take(MAX_LINE_CHARS).collect::<String>()
        )
    } else {
        s.to_string()
    }
}

/// State for a single agent terminal being tracked.
pub struct TrackedTerminal {
    /// Human-readable name (e.g. "Claude Code").
    pub agent_name: SharedString,
    /// Weak handle to the terminal entity to check liveness.
    pub terminal: WeakEntity<Terminal>,
    /// Most recent output lines (max 4).
    pub output_lines: VecDeque<SharedString>,
    /// Whether the terminal task is still running.
    pub is_active: bool,
    /// Timestamp of the last output received.
    pub last_output_at: Instant,
    /// Whether we've received at least one batch of output.
    pub has_output: bool,
}

/// Manager for the terminal agent preview strip.
pub struct TerminalAgentPreview {
    /// Tracked running agent terminals.
    pub tracked: Vec<Option<TrackedTerminal>>,
    /// Subscription handles to terminal `Wakeup` events.
    pub subscriptions: Vec<Subscription>,
    /// Whether the strip is collapsed by the user.
    pub collapsed: bool,
    /// Index into `tracked` of the most recently active terminal.
    pub most_recent_index: usize,
    /// Debounce timestamp — don't re-render more often than DEBOUNCE_INTERVAL.
    pub last_render_time: Instant,
}

impl TerminalAgentPreview {
    pub fn new() -> Self {
        Self {
            tracked: Vec::new(),
            subscriptions: Vec::new(),
            collapsed: false,
            most_recent_index: 0,
            last_render_time: Instant::now(),
        }
    }

    /// Number of currently active (running) agent terminals.
    pub fn active_count(&self) -> usize {
        self.tracked
            .iter()
            .filter(|t| t.as_ref().is_some_and(|t| t.is_active))
            .count()
    }

    /// Total number of tracked terminals (active + recent done).
    pub fn total_count(&self) -> usize {
        self.tracked.len()
    }

    /// The primary terminal to show (most recently active).
    pub fn primary(&self) -> Option<&TrackedTerminal> {
        self.tracked
            .get(self.most_recent_index)
            .and_then(|t| t.as_ref())
    }

    /// Find the index of a tracked terminal by agent name.
    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        self.tracked.iter().position(|t| {
            t.as_ref()
                .is_some_and(|tracked| tracked.agent_name.as_ref() == name)
        })
    }

    /// Remove all expired / dead terminal entries (where the entity is dropped).
    pub fn prune_dead(&mut self, cx: &App) {
        self.tracked.retain(|t| {
            t.as_ref().is_some_and(|tracked| {
                tracked
                    .terminal
                    .upgrade()
                    .map(|term: Entity<Terminal>| {
                        term.read_with(cx, |term: &Terminal, _| term.task().is_some())
                    })
                    .unwrap_or(false)
            })
        });
        self.most_recent_index = self
            .most_recent_index
            .min(self.tracked.len().saturating_sub(1));
    }

    /// Update the output for a terminal and mark it as most recent.
    pub fn update_output(
        &mut self,
        name: &str,
        terminal: WeakEntity<Terminal>,
        lines: Vec<SharedString>,
        is_active: bool,
    ) {
        let now = Instant::now();
        if let Some(idx) = self.index_by_name(name) {
            if let Some(Some(tracked)) = self.tracked.get_mut(idx) {
                tracked.output_lines.clear();
                for line in lines {
                    tracked.output_lines.push_back(line);
                }
                tracked.is_active = is_active;
                tracked.has_output = true;
                tracked.last_output_at = now;
            }
            self.most_recent_index = idx;
        } else {
            let mut output_lines = VecDeque::new();
            for line in lines {
                output_lines.push_back(line);
            }
            self.tracked.push(Some(TrackedTerminal {
                agent_name: SharedString::from(name),
                terminal,
                output_lines,
                is_active,
                last_output_at: now,
                has_output: true,
            }));
            self.most_recent_index = self.tracked.len() - 1;
        }
    }

    /// Mark a terminal as done (no longer active, will fade after DONE_FADE_DURATION).
    pub fn mark_done(&mut self, name: &str) {
        if let Some(idx) = self.index_by_name(name) {
            if let Some(Some(tracked)) = self.tracked.get_mut(idx) {
                tracked.is_active = false;
                tracked.last_output_at = Instant::now();
            }
        }
    }

    /// Whether the preview strip should be visible (has active or recently-done terminals).
    pub fn is_visible(&self) -> bool {
        let now = Instant::now();
        self.tracked.iter().any(|t| {
            t.as_ref().is_some_and(|tracked| {
                tracked.is_active
                    || now.duration_since(tracked.last_output_at) < DONE_FADE_DURATION
            })
        })
    }

    /// Attempt to upgrade a weak terminal ref for a tracked terminal.
    pub fn upgrade_terminal(&self, idx: usize) -> Option<Entity<Terminal>> {
        self.tracked
            .get(idx)
            .and_then(|t| t.as_ref())
            .and_then(|tracked| tracked.terminal.upgrade())
    }

    /// Remove a tracked terminal by name (e.g. when terminal tab is closed).
    pub fn remove(&mut self, name: &str) {
        if let Some(idx) = self.index_by_name(name) {
            self.tracked.remove(idx);
            if !self.tracked.is_empty() {
                self.most_recent_index = self.most_recent_index.min(self.tracked.len() - 1);
            } else {
                self.most_recent_index = 0;
            }
        }
    }

    /// Check if it's too soon to render again (debounce).
    pub fn should_debounce(&self) -> bool {
        Instant::now().duration_since(self.last_render_time) < DEBOUNCE_INTERVAL
    }

    /// Update the debounce timestamp.
    pub fn mark_rendered(&mut self) {
        self.last_render_time = Instant::now();
    }
}
