#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass
from typing import Any

from textual.app import App, ComposeResult
from textual.containers import VerticalScroll
from textual.widgets import Footer, Header, Static


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

    #actions {
        height: auto;
        padding: 0 1 1 1;
        color: $text-muted;
    }

    #content {
        height: 1fr;
        padding: 0 1;
    }

    .section {
        height: auto;
        margin-bottom: 1;
        padding: 1;
        border: round $surface-lighten-2;
    }

    .section-title {
        text-style: bold;
        color: $accent;
    }
    
    .empty {
        color: $text-muted;
    }
    """

    BINDINGS = [
        ("r", "refresh", "Refresh"),
        ("n", "new_task_help", "Create task"),
        ("q", "quit", "Quit"),
    ]

    def __init__(self, client: AjaxClient) -> None:
        super().__init__()
        self.client = client

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Ajax Cockpit", id="summary")
        yield Static("r refresh | n create task | q quit", id="actions")
        with VerticalScroll(id="content"):
            yield Static("", id="repos", classes="section")
            yield Static("", id="inbox", classes="section")
            yield Static("", id="tasks", classes="section")
            yield Static("", id="review", classes="section")
        yield Footer()

    def on_mount(self) -> None:
        self.refresh_data()

    def action_refresh(self) -> None:
        self.refresh_data()

    def action_new_task_help(self) -> None:
        self.query_one("#actions", Static).update(
            'Create task: ajax new --repo <repo> --title "task title" --agent codex --execute'
        )

    def refresh_data(self) -> None:
        repos = self.client.repos()
        tasks = self.client.tasks()
        inbox = self.client.inbox()
        review = self.client.review()

        summary = self.query_one("#summary", Static)
        summary.update(
            f"Ajax Cockpit | repos: {len(repos)} | tasks: {len(tasks)} | "
            f"review: {len(review)} | inbox: {len(inbox)}"
        )

        self.query_one("#actions", Static).update("r refresh | n create task | q quit")
        self.query_one("#repos", Static).update(render_repos(repos))
        self.query_one("#inbox", Static).update(render_inbox(inbox))
        self.query_one("#tasks", Static).update(render_tasks(tasks))
        self.query_one("#review", Static).update(render_review(review))


def render_repos(repos: list[dict[str, Any]]) -> str:
    lines = ["[section-title]Repos[/]"]
    if not repos:
        lines.append("[empty]No repos configured yet.[/]")
        lines.append("Add repos in ~/.config/ajax/config.toml")
        return "\n".join(lines)

    for repo in repos:
        name = str(repo.get("name", ""))
        active = repo.get("active_tasks", 0)
        reviewable = repo.get("reviewable_tasks", 0)
        cleanable = repo.get("cleanable_tasks", 0)
        broken = repo.get("broken_tasks", 0)
        lines.append(f"{name}")
        lines.append(
            f"  active {active} | review {reviewable} | clean {cleanable} | broken {broken}"
        )
    return "\n".join(lines)


def render_inbox(inbox: list[dict[str, Any]]) -> str:
    lines = ["[section-title]Needs Attention[/]"]
    if not inbox:
        lines.append("[empty]Nothing needs attention.[/]")
        return "\n".join(lines)

    for item in inbox:
        task = str(item.get("task_handle", ""))
        reason = str(item.get("reason", ""))
        action = str(item.get("recommended_action", ""))
        lines.append(task)
        lines.append(f"  {reason}")
        lines.append(f"  next: {action}")
    return "\n".join(lines)


def render_tasks(tasks: list[dict[str, Any]]) -> str:
    lines = ["[section-title]Tasks[/]"]
    if not tasks:
        lines.append("[empty]No tasks yet.[/]")
        lines.append('Create task: ajax new --repo <repo> --title "task title" --agent codex')
        return "\n".join(lines)

    for task in tasks:
        handle = str(task.get("qualified_handle", ""))
        status = str(task.get("lifecycle_status", ""))
        title = str(task.get("title", ""))
        flags = task.get("side_flags", [])
        flag_text = f" | flags: {', '.join(flags)}" if flags else ""
        lines.append(f"{handle}")
        lines.append(f"  {status}{flag_text}")
        lines.append(f"  {title}")
    return "\n".join(lines)


def render_review(review: list[dict[str, Any]]) -> str:
    lines = ["[section-title]Review[/]"]
    if not review:
        lines.append("[empty]No reviewable tasks.[/]")
        return "\n".join(lines)

    for task in review:
        handle = str(task.get("qualified_handle", ""))
        title = str(task.get("title", ""))
        lines.append(handle)
        lines.append(f"  {title}")
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Ajax Textual cockpit")
    parser.add_argument("--ajax-bin", default="ajax")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    AjaxTextualApp(AjaxClient(args.ajax_bin)).run()


if __name__ == "__main__":
    main()
