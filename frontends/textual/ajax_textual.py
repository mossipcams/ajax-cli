#!/usr/bin/env python3
from __future__ import annotations

import argparse
from typing import Any

from textual import events
from textual.app import App, ComposeResult
from textual.containers import Vertical
from textual.widgets import Footer, Header, Label, ListItem, ListView, Static

try:
    from ajax_textual_layout import (
        SelectionRow,
        SummaryCounts,
        is_compact_viewport,
        layout_metrics,
        render_detail,
        render_row,
        render_summary,
        startup_error_rows,
    )
    from ajax_textual_client import AjaxClient
except ModuleNotFoundError:
    from frontends.textual.ajax_textual_layout import (
        SelectionRow,
        SummaryCounts,
        is_compact_viewport,
        layout_metrics,
        render_detail,
        render_row,
        render_summary,
        startup_error_rows,
    )
    from frontends.textual.ajax_textual_client import AjaxClient


class AjaxTextualApp(App[None]):
    CSS = """
    Screen {
        layout: vertical;
    }

    #summary {
        height: auto;
        min-height: 3;
        padding: 1;
        text-style: bold;
    }

    #body {
        height: 1fr;
    }

    #items {
        height: 2fr;
        min-height: 12;
        border: round $surface-lighten-2;
    }

    ListItem {
        height: auto;
        min-height: 3;
        padding: 1;
    }

    ListItem.--highlight {
        background: $accent;
        color: $text;
    }

    #details {
        height: 1fr;
        min-height: 7;
        padding: 1;
        border: round $surface-lighten-2;
    }

    Screen.compact Header,
    Screen.compact Footer {
        display: none;
    }

    Screen.compact #summary {
        min-height: 1;
        padding: 0 1;
    }

    Screen.compact #items {
        min-height: 5;
    }

    Screen.compact ListItem {
        min-height: 2;
        padding: 0 1;
    }

    Screen.compact #details {
        min-height: 4;
        padding: 0 1;
    }
    """

    BINDINGS = [
        ("r", "refresh", "Refresh"),
        ("enter", "select_cursor", "Select"),
        ("q", "quit", "Quit"),
    ]

    def __init__(self, client: AjaxClient) -> None:
        super().__init__()
        self.client = client
        self.rows: list[SelectionRow] = []
        self.compact = False
        self.summary_counts = SummaryCounts.empty()

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Ajax", id="summary")
        with Vertical(id="body"):
            yield ListView(id="items")
            yield Static("Select a row.", id="details")
        yield Footer()

    def on_mount(self) -> None:
        self.update_viewport_mode(self.size.width, self.size.height)
        self.refresh_data()

    def on_resize(self, event: events.Resize) -> None:
        self.update_viewport_mode(event.size.width, event.size.height)
        self.refresh_rendered_rows()
        self.refresh_summary()
        self.refresh_detail()

    def action_refresh(self) -> None:
        self.refresh_data()

    def action_select_cursor(self) -> None:
        list_view = self.query_one("#items", ListView)
        index = list_view.index
        if index is not None:
            self.show_detail(index)

    def on_list_view_highlighted(self, event: ListView.Highlighted) -> None:
        if event.list_view.id == "items" and event.item is not None:
            index = event.list_view.index
            if index is not None:
                self.show_detail(index)

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        if event.list_view.id == "items":
            index = event.list_view.index
            if index is not None:
                self.show_detail(index)

    def refresh_data(self) -> None:
        try:
            snapshot = self.client.snapshot()
        except Exception as error:
            self.rows = startup_error_rows(error)
            self.summary_counts = SummaryCounts.empty()
            self.refresh_summary()
            self.refresh_rendered_rows()
            self.query_one("#items", ListView).index = 0
            self.show_detail(0)
            return

        repos = snapshot.repos
        tasks = snapshot.tasks
        inbox = snapshot.inbox
        review = snapshot.review
        self.rows = build_selection_rows(repos, tasks, inbox, review)
        self.summary_counts = SummaryCounts(
            repo_count=len(repos),
            task_count=len(tasks),
            review_count=len(review),
            inbox_count=len(inbox),
        )

        self.refresh_summary()
        self.refresh_rendered_rows()

        list_view = self.query_one("#items", ListView)
        if self.rows:
            list_view.index = 0
            self.show_detail(0)
        else:
            self.query_one("#details", Static).update("No Ajax data available.")

    def refresh_rendered_rows(self) -> None:
        list_view = self.query_one("#items", ListView)
        list_view.clear()
        for row in self.rows:
            list_view.append(
                ListItem(
                    Label(
                        render_row(
                            row,
                            compact=self.compact,
                            width=self.content_width(),
                        )
                    )
                )
            )

    def refresh_summary(self) -> None:
        self.query_one("#summary", Static).update(
            render_summary(
                repo_count=self.summary_counts.repo_count,
                task_count=self.summary_counts.task_count,
                review_count=self.summary_counts.review_count,
                inbox_count=self.summary_counts.inbox_count,
                compact=self.compact,
            )
        )

    def refresh_detail(self) -> None:
        list_view = self.query_one("#items", ListView)
        index = list_view.index
        if index is not None:
            self.show_detail(index)

    def update_viewport_mode(self, width: int, height: int) -> None:
        compact = is_compact_viewport(width, height)
        if compact == self.compact:
            return

        self.compact = compact
        self.screen.set_class(compact, "compact")
        metrics = layout_metrics(compact=compact)
        for selector in ("Header", "Footer"):
            for widget in self.query(selector):
                widget.display = metrics.show_header_footer

    def show_detail(self, index: int) -> None:
        if index < 0 or index >= len(self.rows):
            return
        self.query_one("#details", Static).update(
            render_detail(
                self.rows[index],
                compact=self.compact,
                width=self.content_width(),
            )
        )

    def content_width(self) -> int:
        return max(self.size.width - 2, 20)


def build_selection_rows(
    repos: list[dict[str, Any]],
    tasks: list[dict[str, Any]],
    inbox: list[dict[str, Any]],
    review: list[dict[str, Any]],
) -> list[SelectionRow]:
    rows = [
        SelectionRow(
            kind="create",
            title="Create task",
            subtitle="Pick this, then run the command shown below.",
            detail='ajax new --repo <repo> --title "task title" --agent codex --execute',
        ),
        SelectionRow(
            kind="refresh",
            title="Refresh",
            subtitle="Reload repos, tasks, inbox, and review queues.",
            detail="Press r to refresh the cockpit.",
        ),
    ]

    rows.extend(repo_rows(repos))
    rows.extend(inbox_rows(inbox))
    rows.extend(task_rows(tasks))
    rows.extend(review_rows(review))
    return rows


def repo_rows(repos: list[dict[str, Any]]) -> list[SelectionRow]:
    if not repos:
        return [
            SelectionRow(
                kind="empty",
                title="No repos configured",
                subtitle="Ajax cannot create tasks until repos are configured.",
                detail="Edit ~/.config/ajax/config.toml and add [[repos]] entries.",
            )
        ]

    rows = [section_row("Repos")]
    for repo in repos:
        name = str(repo.get("name", ""))
        active = repo.get("active_tasks", 0)
        reviewable = repo.get("reviewable_tasks", 0)
        cleanable = repo.get("cleanable_tasks", 0)
        broken = repo.get("broken_tasks", 0)
        rows.append(
            SelectionRow(
                kind="repo",
                title=name,
                subtitle=f"active {active} | review {reviewable} | clean {cleanable} | broken {broken}",
                detail=(
                    f"Repo: {name}\n\n"
                    f"Create task:\najax new --repo {name} --title \"task title\" --agent codex --execute\n\n"
                    f"List tasks:\najax tasks --repo {name}"
                ),
            )
        )
    return rows


def inbox_rows(inbox: list[dict[str, Any]]) -> list[SelectionRow]:
    rows = [section_row("Needs attention")]
    if not inbox:
        rows.append(
            SelectionRow(
                kind="empty",
                title="Nothing needs attention",
                subtitle="No blocked, stale, dirty, or reviewable items.",
                detail="You are clear right now. Refresh after agents run.",
            )
        )
        return rows

    for item in inbox:
        task = str(item.get("task_handle", ""))
        reason = str(item.get("reason", ""))
        action = str(item.get("recommended_action", ""))
        rows.append(
            SelectionRow(
                kind="inbox",
                title=task,
                subtitle=reason,
                detail=f"{task}\n\nReason:\n{reason}\n\nRecommended action:\n{action}",
            )
        )
    return rows


def task_rows(tasks: list[dict[str, Any]]) -> list[SelectionRow]:
    rows = [section_row("Tasks")]
    if not tasks:
        rows.append(
            SelectionRow(
                kind="empty",
                title="No tasks yet",
                subtitle="Select Create task or a repo row to see the command.",
                detail='Create task:\najax new --repo <repo> --title "task title" --agent codex --execute',
            )
        )
        return rows

    for task in tasks:
        handle = str(task.get("qualified_handle", ""))
        status = str(task.get("lifecycle_status", ""))
        title = str(task.get("title", ""))
        flags = task.get("side_flags", [])
        flag_text = f"\nFlags: {', '.join(flags)}" if flags else ""
        rows.append(
            SelectionRow(
                kind="task",
                title=handle,
                subtitle=f"{status} | {title}",
                detail=(
                    f"{handle}\n\n"
                    f"Status: {status}{flag_text}\n"
                    f"Title: {title}\n\n"
                    f"Open:\najax open {handle} --execute\n\n"
                    f"Inspect:\najax inspect {handle}"
                ),
            )
        )
    return rows


def review_rows(review: list[dict[str, Any]]) -> list[SelectionRow]:
    rows = [section_row("Review")]
    if not review:
        rows.append(
            SelectionRow(
                kind="empty",
                title="No reviewable tasks",
                subtitle="Tasks marked reviewable will appear here.",
                detail="Review queue is empty.",
            )
        )
        return rows

    for task in review:
        handle = str(task.get("qualified_handle", ""))
        title = str(task.get("title", ""))
        rows.append(
            SelectionRow(
                kind="review",
                title=handle,
                subtitle=title,
                detail=(
                    f"{handle}\n\n"
                    f"{title}\n\n"
                    f"Diff:\najax diff {handle} --execute\n\n"
                    f"Merge:\najax merge {handle} --execute --yes"
                ),
            )
        )
    return rows


def section_row(title: str) -> SelectionRow:
    return SelectionRow(kind="section", title=f"-- {title} --", subtitle="", detail=title)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Ajax Textual cockpit")
    parser.add_argument("--ajax-bin", default="ajax")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    AjaxTextualApp(AjaxClient(args.ajax_bin)).run()


if __name__ == "__main__":
    main()
