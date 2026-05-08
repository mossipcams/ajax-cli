from __future__ import annotations

import unittest

from frontends.textual.ajax_textual_client import AjaxClient
from frontends.textual.ajax_textual_layout import (
    SelectionRow,
    compact_summary,
    is_compact_viewport,
    layout_metrics,
    render_detail,
    render_row,
    startup_error_rows,
)


class AjaxClientStartupTests(unittest.TestCase):
    def test_snapshot_loads_startup_data_with_one_command(self) -> None:
        calls: list[list[str]] = []

        def fake_command(command: list[str]) -> str:
            calls.append(command)
            return (
                '{"repos":{"repos":[{"name":"web"}]},'
                '"tasks":{"tasks":[{"qualified_handle":"web/fix-login"}]},'
                '"review":{"tasks":[]},'
                '"inbox":{"items":[{"task_handle":"web/fix-login"}]}}'
            )

        client = AjaxClient("ajax", command_runner=fake_command)
        snapshot = client.snapshot()

        self.assertEqual(calls, [["ajax", "cockpit", "--json"]])
        self.assertEqual(snapshot.repos, [{"name": "web"}])
        self.assertEqual(snapshot.tasks, [{"qualified_handle": "web/fix-login"}])
        self.assertEqual(snapshot.review, [])
        self.assertEqual(snapshot.inbox, [{"task_handle": "web/fix-login"}])


class ResponsiveRenderingTests(unittest.TestCase):
    def test_startup_error_rows_show_refresh_guidance(self) -> None:
        rows = startup_error_rows(RuntimeError("ajax failed"))

        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0].kind, "error")
        self.assertIn("Ajax data failed to load", rows[0].title)
        self.assertIn("Press r", rows[0].detail)
        self.assertIn("ajax failed", rows[0].detail)

    def test_compact_viewport_matches_narrow_or_short_terminals(self) -> None:
        self.assertTrue(is_compact_viewport(70, 30))
        self.assertTrue(is_compact_viewport(100, 20))
        self.assertFalse(is_compact_viewport(100, 30))

    def test_compact_summary_keeps_counts_scannable(self) -> None:
        self.assertEqual(
            compact_summary(repo_count=3, task_count=12, review_count=2, inbox_count=4),
            "Ajax  R3 T12 V2 I4",
        )

    def test_compact_layout_reduces_chrome_and_minimum_heights(self) -> None:
        compact = layout_metrics(compact=True)
        full = layout_metrics(compact=False)

        self.assertLess(compact.summary_min_height, full.summary_min_height)
        self.assertLess(compact.items_min_height, full.items_min_height)
        self.assertLess(compact.details_min_height, full.details_min_height)
        self.assertLess(compact.row_min_height, full.row_min_height)
        self.assertFalse(compact.show_header_footer)
        self.assertTrue(full.show_header_footer)

    def test_compact_rows_preserve_title_and_truncate_subtitle(self) -> None:
        row = SelectionRow(
            kind="task",
            title="web/fix-login",
            subtitle="running | tighten the responsive cockpit layout on small devices",
            detail="unused",
        )

        rendered = render_row(row, compact=True, width=34)

        self.assertEqual(rendered, "web/fix-login\nrunning | tighten the respo...")
        self.assertLessEqual(max(len(line) for line in rendered.splitlines()), 34)

    def test_compact_details_keep_action_commands_visible(self) -> None:
        row = SelectionRow(
            kind="task",
            title="web/fix-login",
            subtitle="running | tighten the responsive cockpit layout on small devices",
            detail=(
                "web/fix-login\n\n"
                "Status: running\n"
                "Title: tighten the responsive cockpit layout on small devices\n\n"
                "Open:\najax open web/fix-login --execute\n\n"
                "Inspect:\najax inspect web/fix-login"
            ),
        )

        rendered = render_detail(row, compact=True, width=38)

        self.assertIn("web/fix-login", rendered)
        self.assertIn("ajax open web/fix-login --execute", rendered)
        self.assertLessEqual(max(len(line) for line in rendered.splitlines()), 38)


if __name__ == "__main__":
    unittest.main()
