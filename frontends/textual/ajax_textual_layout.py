from __future__ import annotations

import textwrap
from dataclasses import dataclass


COMPACT_WIDTH = 80
COMPACT_HEIGHT = 24
MIN_WRAP_WIDTH = 20
COMPACT_ROW_CHROME_WIDTH = 6


@dataclass(frozen=True)
class SelectionRow:
    kind: str
    label: str
    title: str
    subtitle: str
    meta: str
    detail: str
    tone: str
    status: str = ""
    actions: list[str] | None = None


@dataclass(frozen=True)
class DashboardSection:
    title: str
    rows: list[SelectionRow]


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


def viewport_layout(width: int, height: int) -> str:
    if is_compact_viewport(width, height) or width < 100:
        return "stacked"
    return "split"


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
        f"Ajax | attention {inbox_count} | review {review_count} | "
        f"tasks {task_count} | repos {repo_count}"
    )


def compact_summary(
    *,
    repo_count: int,
    task_count: int,
    review_count: int,
    inbox_count: int,
) -> str:
    return f"Ajax  attention {inbox_count}  review {review_count}  tasks {task_count}  repos {repo_count}"


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
    heading = "  ".join(part for part in [row.label, row.title] if part)
    meta_line = "  ".join(part for part in [row.meta, row.subtitle] if part)

    if compact:
        content_width = usable_compact_row_width(width)
        return "\n".join(
            [
                fit_line(heading, content_width),
                fit_line(meta_line, content_width),
            ]
        )

    if meta_line:
        return f"{heading}\n{meta_line}"
    return heading


def usable_compact_row_width(width: int | None) -> int | None:
    if width is None:
        return None
    return max(width - COMPACT_ROW_CHROME_WIDTH, MIN_WRAP_WIDTH)


def render_detail(row: SelectionRow, *, compact: bool = False, width: int | None = None) -> str:
    detail = structured_detail(row)
    if not compact:
        return detail

    detail_width = max(width or COMPACT_WIDTH, MIN_WRAP_WIDTH)
    wrapped_blocks = [
        wrap_preserving_commands(block, detail_width)
        for block in detail.split("\n\n")
        if block.strip()
    ]
    return "\n\n".join(wrapped_blocks)


def structured_detail(row: SelectionRow) -> str:
    parts = [f"Context\n{row.title}"]
    if row.status or row.meta:
        parts.append(f"Status\n{row.status or row.meta}")
    if row.subtitle:
        parts.append(f"Notes\n{row.subtitle}")
    actions = row.actions or []
    if actions:
        parts.append("Actions\n" + "\n".join(actions))
    elif row.detail:
        parts.append(row.detail)
    return "\n\n".join(parts)


def startup_error_rows(error: Exception) -> list[SelectionRow]:
    message = str(error) or error.__class__.__name__
    return [
        SelectionRow(
            kind="error",
            label="error",
            title="Ajax data failed to load",
            subtitle="Press r to retry after fixing the backend issue.",
            meta="startup",
            detail=f"Ajax data failed to load.\n\n{message}\n\nPress r to retry.",
            tone="danger",
            status="startup failed",
            actions=["Press r to retry"],
        )
    ]


def build_dashboard_sections(
    repos: list[dict],
    tasks: list[dict],
    inbox: list[dict],
    review: list[dict],
) -> list[DashboardSection]:
    sections: list[DashboardSection] = []

    attention = attention_rows(inbox)
    if attention:
        sections.append(DashboardSection("Attention", attention))

    reviewable = review_task_rows(review)
    if reviewable:
        sections.append(DashboardSection("Review", reviewable))

    active = active_task_rows(tasks)
    if active:
        sections.append(DashboardSection("Active", active))

    repo_section = repo_dashboard_rows(repos)
    if repo_section:
        sections.append(DashboardSection("Repos", repo_section))

    if not sections:
        sections.append(
            DashboardSection(
                "Start",
                [
                    SelectionRow(
                        kind="empty",
                        label="ready",
                        title="No Ajax activity yet",
                        subtitle="Create a task from a configured repo when you are ready.",
                        meta="idle",
                        detail='Create task:\najax new --repo <repo> --title "task title" --agent codex --execute',
                        tone="muted",
                        status="idle",
                        actions=[
                            'ajax new --repo <repo> --title "task title" --agent codex --execute'
                        ],
                    )
                ],
            )
        )

    return sections


def attention_rows(inbox: list[dict]) -> list[SelectionRow]:
    rows = []
    for item in inbox:
        task = str(item.get("task_handle", ""))
        reason = str(item.get("reason", ""))
        action = str(item.get("recommended_action", ""))
        rows.append(
            SelectionRow(
                kind="inbox",
                label="attention",
                title=task,
                subtitle=reason,
                meta="needs input",
                detail=f"Reason:\n{reason}",
                tone="urgent",
                status=reason,
                actions=[action] if action else [],
            )
        )
    return rows


def review_task_rows(review: list[dict]) -> list[SelectionRow]:
    rows = []
    for task in review:
        handle = str(task.get("qualified_handle", ""))
        title = str(task.get("title", ""))
        rows.append(
            SelectionRow(
                kind="review",
                label="review",
                title=handle,
                subtitle=title,
                meta="ready",
                detail=title,
                tone="review",
                status="ready for review",
                actions=[
                    f"ajax diff {handle} --execute",
                    f"ajax merge {handle} --execute --yes",
                ],
            )
        )
    return rows


def active_task_rows(tasks: list[dict]) -> list[SelectionRow]:
    rows = []
    for task in tasks:
        handle = str(task.get("qualified_handle", ""))
        status = str(task.get("lifecycle_status", ""))
        title = str(task.get("title", ""))
        if status in {"reviewable", "mergeable"}:
            continue
        rows.append(
            SelectionRow(
                kind="task",
                label="task",
                title=handle,
                subtitle=title,
                meta=status,
                detail=title,
                tone="neutral",
                status=status,
                actions=[
                    f"ajax open {handle} --execute",
                    f"ajax inspect {handle}",
                ],
            )
        )
    return rows


def repo_dashboard_rows(repos: list[dict]) -> list[SelectionRow]:
    if not repos:
        return [
            SelectionRow(
                kind="empty",
                label="setup",
                title="No repos configured",
                subtitle="Ajax needs repos before it can create task environments.",
                meta="config",
                detail="Edit ~/.config/ajax/config.toml and add [[repos]] entries.",
                tone="muted",
                status="not configured",
                actions=["Edit ~/.config/ajax/config.toml"],
            )
        ]

    rows = []
    for repo in repos:
        name = str(repo.get("name", ""))
        active = repo.get("active_tasks", 0)
        reviewable = repo.get("reviewable_tasks", 0)
        cleanable = repo.get("cleanable_tasks", 0)
        broken = repo.get("broken_tasks", 0)
        rows.append(
            SelectionRow(
                kind="repo",
                label="repo",
                title=name,
                subtitle=f"review {reviewable}  clean {cleanable}  broken {broken}",
                meta=f"active {active}",
                detail=f"Repo: {name}",
                tone="neutral",
                status=f"active {active}, review {reviewable}, clean {cleanable}, broken {broken}",
                actions=[
                    f'ajax new --repo {name} --title "task title" --agent codex --execute',
                    f"ajax tasks --repo {name}",
                ],
            )
        )
    return rows


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
