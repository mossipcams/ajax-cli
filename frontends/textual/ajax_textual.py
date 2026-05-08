#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass
from typing import Any

from textual.app import App, ComposeResult
from textual.containers import Vertical
from textual.widgets import Footer, Header, Label, ListItem, ListView, Static


@dataclass(frozen=True)
class AjaxClient:
    ajax_bin: str

    def json_command(self, *args: str) -> dict[str, Any]:
        command = [self.ajax_bin, *args, "--json"]
        output = subprocess.check_output(command, text=True)
        loaded = json.loads(output)
        if not isinstance(loaded, dict):
            raise ValueError(f"Ajax returned non-object JSON for {' '.join(command)}")
        return loaded

    def repos(self) -> list[dict[str, Any]]:
        return list(self.json_command("repos").get("repos", []))

    def tasks(self) -> list[dict[str, Any]]:
        return list(self.json_command("tasks").get("tasks", []))

    def inbox(self) -> list[dict[str, Any]]:
        return list(self.json_command("inbox").get("items", []))

    def review(self) -> list[dict[str, Any]]:
        return list(self.json_command("review").get("tasks", []))


@dataclass(frozen=True)
class SelectionRow:
    kind: str
    title: str
    subtitle: str
    detail: str


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

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Ajax", id="summary")
        with Vertical(id="body"):
            yield ListView(id="items")
            yield Static("Select a row.", id="details")
        yield Footer()

    def on_mount(self) -> None:
        self.refresh_data()

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
        repos = self.client.repos()
        tasks = self.client.tasks()
        inbox = self.client.inbox()
        review = self.client.review()
        self.rows = build_selection_rows(repos, tasks, inbox, review)

        self.query_one("#summary", Static).update(
            f"Ajax | repos {len(repos)} | tasks {len(tasks)} | "
            f"review {len(review)} | inbox {len(inbox)}"
        )

        list_view = self.query_one("#items", ListView)
        list_view.clear()
        for row in self.rows:
            list_view.append(ListItem(Label(render_row(row))))

        if self.rows:
            list_view.index = 0
            self.show_detail(0)
        else:
            self.query_one("#details", Static).update("No Ajax data available.")

    def show_detail(self, index: int) -> None:
        if index < 0 or index >= len(self.rows):
            return
        self.query_one("#details", Static).update(self.rows[index].detail)


def render_row(row: SelectionRow) -> str:
    if row.subtitle:
        return f"{row.title}\n{row.subtitle}"
    return row.title


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
