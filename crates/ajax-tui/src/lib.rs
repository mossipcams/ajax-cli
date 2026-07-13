#![deny(unsafe_op_in_unsafe_fn)]

mod actions;
mod cockpit_state;
mod input;
mod layout;
mod navigation;
mod palette;
mod rendering;
mod runtime;

#[cfg(test)]
mod architecture;

use ajax_core::{
    models::CockpitActionItem,
    output::{InboxResponse, ReposResponse, TaskCard},
};
pub use cockpit_state::{App, CockpitSnapshot};
#[cfg(test)]
use cockpit_state::{AppView, SelectableKind, Severity};
#[cfg(test)]
use input::{handle_action_result, handle_cockpit_event, EventLoopAction};
pub(crate) use layout::{feed_screen_rows, feed_top_row, selectable_row_layout};
use rendering::task_status_text;
#[cfg(test)]
use rendering::{
    action_glyph, bucket_color, bucket_glyph, priority_accent, project_subtitle, render_ui,
    task_glyph, StatusBucket,
};
pub use runtime::run_interactive_with_flash_and_refresh;
use std::io;

// ── Text renderer (watch mode) ────────────────────────────────────────────────

pub fn render_cockpit(repos: &ReposResponse, cards: &[TaskCard], inbox: &InboxResponse) -> String {
    let mut lines = vec![
        "Ajax Cockpit".to_string(),
        format!("Repos: {}", repos.repos.len()),
        format!("Tasks: {}", cards.len()),
        "Task Statuses".to_string(),
    ];

    if cards.is_empty() {
        lines.push("no active tasks".to_string());
    } else {
        lines.extend(cards.iter().map(|card| {
            format!(
                "{}\t{}\t{}",
                card.qualified_handle,
                task_status_text(card),
                card.title
            )
        }));
    }

    lines.push("Inbox".to_string());

    if inbox.items.is_empty() {
        lines.push("no tasks need attention".to_string());
    } else {
        lines.extend(inbox.items.iter().map(|item| {
            format!(
                "{}: {} -> {}",
                item.task_handle,
                item.reason,
                item.action.as_str()
            )
        }));
    }

    lines.join("\n")
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Returned when the TUI exits with a deferred action (e.g. open → tmux attach).
pub struct PendingAction {
    pub task_handle: String,
    pub action: String,
    pub task_title: Option<String>,
}

/// What the `on_action` callback returns to tell the TUI what to do next.
pub enum ActionOutcome {
    /// Reload the TUI with fresh data.
    Refresh(CockpitSnapshot),
    /// Reload the TUI optimistically, then exit to run a deferred action.
    RefreshAndDefer(CockpitSnapshot, PendingAction),
    /// Exit the TUI — the CLI will run the deferred action.
    Defer(PendingAction),
    /// Ask for a second explicit activation before running a risky action.
    Confirm(String),
    /// Show a brief status message then stay in the TUI.
    Message(String),
}

pub trait CockpitEventHandler {
    fn on_action(&mut self, item: &CockpitActionItem) -> io::Result<ActionOutcome>;

    fn on_confirmed_action(&mut self, item: &CockpitActionItem) -> io::Result<ActionOutcome> {
        self.on_action(item)
    }

    fn on_refresh(&mut self) -> io::Result<Option<CockpitSnapshot>> {
        Ok(None)
    }
}

// ── Layout-coupled state helpers ──────────────────────────────────────────────

impl App {
    /// Select whichever selectable occupies the given absolute feed row.
    /// No-op if the row falls on a section header / placeholder.
    pub fn select_at_feed_row(&mut self, feed_row: usize) {
        let layout = selectable_row_layout(self);
        if let Some((idx, _)) = layout
            .iter()
            .enumerate()
            .find(|(_, r)| r.contains(&feed_row))
        {
            self.selected = idx;
        }
    }

    /// Adjust viewport so the selected item is visible within `viewport_h` rows.
    fn ensure_visible(&mut self, viewport_h: usize) {
        if viewport_h == 0 {
            return;
        }
        let layout = selectable_row_layout(self);
        let Some(range) = layout.get(self.selected).cloned() else {
            return;
        };
        if range.start < self.viewport_scroll {
            self.viewport_scroll = range.start;
        }
        let bottom = self.viewport_scroll + viewport_h;
        if range.end > bottom {
            self.viewport_scroll = range.end.saturating_sub(viewport_h);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
