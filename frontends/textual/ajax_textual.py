#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass
from typing import Any

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import DataTable, Footer, Header, Static


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
        height: 3;
        padding: 0 1;
    }

    #main {
        height: 1fr;
    }

    DataTable {
        height: 1fr;
    }
    """

    BINDINGS = [
        ("r", "refresh", "Refresh"),
        ("q", "quit", "Quit"),
    ]

    def __init__(self, client: AjaxClient) -> None:
        super().__init__()
        self.client = client

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Ajax Cockpit", id="summary")
        with Horizontal(id="main"):
            with Vertical():
                yield Static("Tasks")
                yield DataTable(id="tasks")
            with Vertical():
                yield Static("Inbox")
                yield DataTable(id="inbox")
        yield Footer()

    def on_mount(self) -> None:
        self._setup_tables()
        self.refresh_data()

    def action_refresh(self) -> None:
        self.refresh_data()

    def _setup_tables(self) -> None:
        tasks = self.query_one("#tasks", DataTable)
        tasks.add_columns("Task", "Status", "Title")
        inbox = self.query_one("#inbox", DataTable)
        inbox.add_columns("Task", "Reason", "Action")

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

        task_table = self.query_one("#tasks", DataTable)
        task_table.clear()
        for task in tasks:
            task_table.add_row(
                str(task.get("qualified_handle", "")),
                str(task.get("lifecycle_status", "")),
                str(task.get("title", "")),
            )

        inbox_table = self.query_one("#inbox", DataTable)
        inbox_table.clear()
        for item in inbox:
            inbox_table.add_row(
                str(item.get("task_handle", "")),
                str(item.get("reason", "")),
                str(item.get("recommended_action", "")),
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Ajax Textual cockpit")
    parser.add_argument("--ajax-bin", default="ajax")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    AjaxTextualApp(AjaxClient(args.ajax_bin)).run()


if __name__ == "__main__":
    main()
