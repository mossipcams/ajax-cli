#!/usr/bin/env python3
from __future__ import annotations

import argparse
from typing import Any

from textual import events
from textual.app import App, ComposeResult
from textual.containers import Container
from textual.widgets import Footer, Header, Label, ListItem, ListView, Static

try:
    from ajax_textual_layout import (
        SelectionRow,
        SummaryCounts,
        build_dashboard_sections,
        is_compact_viewport,
        layout_metrics,
        render_detail,
        render_row,
        render_summary,
        startup_error_rows,
        viewport_layout,
    )
    from ajax_textual_client import AjaxClient
except ModuleNotFoundError:
    from frontends.textual.ajax_textual_layout import (
        SelectionRow,
        SummaryCounts,
        build_dashboard_sections,
        is_compact_viewport,
        layout_metrics,
        render_detail,
        render_row,
        render_summary,
        startup_error_rows,
        viewport_layout,
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
        padding: 0 1;
        text-style: bold;
        background: $boost;
    }

    #body {
        height: 1fr;
        layout: horizontal;
    }

    #items {
        width: 2fr;
        height: 1fr;
        min-height: 12;
        border: solid $surface-lighten-2;
    }

    ListItem {
        height: auto;
        min-height: 3;
        padding: 1;
    }

    ListItem.urgent {
        border-left: thick $error;
    }

    ListItem.review {
        border-left: thick $warning;
    }

    ListItem.muted {
        color: $text-muted;
    }

    ListItem.--highlight {
        background: $accent;
        color: $text;
    }

    #details {
        width: 3fr;
        height: 1fr;
        min-height: 7;
        padding: 1;
        border: solid $surface-lighten-2;
    }

    Screen.compact Header,
    Screen.compact Footer {
        display: none;
    }

    Screen.compact #summary {
        min-height: 1;
        padding: 0 1;
    }

    Screen.compact #body {
        layout: vertical;
    }

    Screen.compact #items {
        width: 1fr;
        height: 2fr;
        min-height: 5;
    }

    Screen.compact ListItem {
        min-height: 2;
        padding: 0 1;
    }

    Screen.compact #details {
        width: 1fr;
        height: 1fr;
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
        self.layout_mode = "split"
        self.summary_counts = SummaryCounts.empty()

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Ajax", id="summary")
        with Container(id="body"):
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
            item = ListItem(
                Label(
                    render_row(
                        row,
                        compact=self.compact,
                        width=self.list_content_width(),
                    )
                )
            )
            for tone in ("urgent", "review", "muted"):
                item.set_class(row.tone == tone, tone)
            list_view.append(item)

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
        self.layout_mode = viewport_layout(width, height)
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

    def list_content_width(self) -> int:
        if self.layout_mode == "split":
            return max((self.size.width * 2 // 5) - 2, 20)
        return self.content_width()


def build_selection_rows(
    repos: list[dict[str, Any]],
    tasks: list[dict[str, Any]],
    inbox: list[dict[str, Any]],
    review: list[dict[str, Any]],
) -> list[SelectionRow]:
    sections = build_dashboard_sections(repos, tasks, inbox, review)
    return [row for section in sections for row in section.rows]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Ajax Textual cockpit")
    parser.add_argument("--ajax-bin", default="ajax")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    AjaxTextualApp(AjaxClient(args.ajax_bin)).run()


if __name__ == "__main__":
    main()
