from __future__ import annotations

import textwrap
from dataclasses import dataclass


COMPACT_WIDTH = 80
COMPACT_HEIGHT = 24
MIN_WRAP_WIDTH = 20
COMPACT_ROW_CHROME_WIDTH = 4


@dataclass(frozen=True)
class SelectionRow:
    kind: str
    title: str
    subtitle: str
    detail: str


@dataclass(frozen=True)
class LayoutMetrics:
    summary_min_height: int
    items_min_height: int
    details_min_height: int
    row_min_height: int
    show_header_footer: bool


@dataclass(frozen=True)
class SummaryCounts:
    repo_count: int
    task_count: int
    review_count: int
    inbox_count: int

    @classmethod
    def empty(cls) -> SummaryCounts:
        return cls(repo_count=0, task_count=0, review_count=0, inbox_count=0)


def is_compact_viewport(width: int, height: int) -> bool:
    return width < COMPACT_WIDTH or height < COMPACT_HEIGHT


def layout_metrics(*, compact: bool) -> LayoutMetrics:
    if compact:
        return LayoutMetrics(
            summary_min_height=1,
            items_min_height=5,
            details_min_height=4,
            row_min_height=2,
            show_header_footer=False,
        )

    return LayoutMetrics(
        summary_min_height=3,
        items_min_height=12,
        details_min_height=7,
        row_min_height=3,
        show_header_footer=True,
    )


def full_summary(
    *,
    repo_count: int,
    task_count: int,
    review_count: int,
    inbox_count: int,
) -> str:
    return (
        f"Ajax | repos {repo_count} | tasks {task_count} | "
        f"review {review_count} | inbox {inbox_count}"
    )


def compact_summary(
    *,
    repo_count: int,
    task_count: int,
    review_count: int,
    inbox_count: int,
) -> str:
    return f"Ajax  R{repo_count} T{task_count} V{review_count} I{inbox_count}"


def render_summary(
    *,
    repo_count: int,
    task_count: int,
    review_count: int,
    inbox_count: int,
    compact: bool,
) -> str:
    renderer = compact_summary if compact else full_summary
    return renderer(
        repo_count=repo_count,
        task_count=task_count,
        review_count=review_count,
        inbox_count=inbox_count,
    )


def render_row(row: SelectionRow, *, compact: bool = False, width: int | None = None) -> str:
    if not row.subtitle:
        return fit_line(row.title, width)

    if compact:
        content_width = usable_compact_row_width(width)
        return "\n".join(
            [
                fit_line(row.title, content_width),
                fit_line(row.subtitle, content_width),
            ]
        )

    return f"{row.title}\n{row.subtitle}"


def usable_compact_row_width(width: int | None) -> int | None:
    if width is None:
        return None
    return max(width - COMPACT_ROW_CHROME_WIDTH, MIN_WRAP_WIDTH)


def render_detail(row: SelectionRow, *, compact: bool = False, width: int | None = None) -> str:
    if not compact:
        return row.detail

    detail_width = max(width or COMPACT_WIDTH, MIN_WRAP_WIDTH)
    wrapped_blocks = [
        wrap_preserving_commands(block, detail_width)
        for block in row.detail.split("\n\n")
        if block.strip()
    ]
    return "\n\n".join(wrapped_blocks)


def startup_error_rows(error: Exception) -> list[SelectionRow]:
    message = str(error) or error.__class__.__name__
    return [
        SelectionRow(
            kind="error",
            title="Ajax data failed to load",
            subtitle="Press r to retry after fixing the backend issue.",
            detail=f"Ajax data failed to load.\n\n{message}\n\nPress r to retry.",
        )
    ]


def fit_line(value: str, width: int | None) -> str:
    if width is None:
        return value

    line_width = max(width, MIN_WRAP_WIDTH)
    if len(value) <= line_width:
        return value
    return f"{value[: line_width - 3]}..."


def wrap_preserving_commands(block: str, width: int) -> str:
    lines = []
    for line in block.splitlines():
        if line.startswith("ajax "):
            lines.append(fit_line(line, width))
            continue
        lines.extend(textwrap.wrap(line, width=width) or [""])
    return "\n".join(lines)
