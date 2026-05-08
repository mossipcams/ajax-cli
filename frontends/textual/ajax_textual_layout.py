from __future__ import annotations

import textwrap
from dataclasses import dataclass

from rich.markup import escape


COMPACT_WIDTH = 80
COMPACT_HEIGHT = 24
MIN_WRAP_WIDTH = 20
COMPACT_ROW_CHROME_WIDTH = 6

_TONE_COLOR = {"urgent": "red", "review": "yellow"}
_KIND_LABEL = {
    "inbox": "ATTN",
    "review": "REVIEW",
    "task": "TASK",
    "repo": "REPO",
    "error": "ERR",
    "empty": "INFO",
}
_TONE_ASCII = {"urgent": "(!)", "review": "(R)"}


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


def render_summary(
    *,
    repo_count: int,
    task_count: int,
    review_count: int,
    inbox_count: int,
    compact: bool,
) -> str:
    if compact:
        return (
            f"Ajax  {inbox_count} attn  {review_count} review  "
            f"{task_count} tasks  {repo_count} repos"
        )

    attn = (
        f"[bold red]● {inbox_count} attention[/bold red]"
        if inbox_count
        else f"[dim]● 0 attention[/dim]"
    )
    rev = (
        f"[bold yellow]● {review_count} review[/bold yellow]"
        if review_count
        else f"[dim]● 0 review[/dim]"
    )
    tasks = f"[dim]●[/dim] {task_count} tasks"
    repos = f"[dim]●[/dim] {repo_count} repos"
    return f"[bold]Ajax[/bold]  {attn}  {rev}  {tasks}  {repos}"


def render_row(row: SelectionRow, *, compact: bool = False, width: int | None = None) -> str:
    if row.kind == "section":
        name = row.label.upper()
        if compact:
            return f"─ {name}"
        return f"[dim]── {name}[/dim]"

    if compact:
        badge = _TONE_ASCII.get(row.tone, "( )")
        heading = f"{badge} {row.title}"
        meta_line = row.meta
        content_width = usable_compact_row_width(width)
        return "\n".join([
            fit_line(heading, content_width),
            fit_line(meta_line, content_width),
        ])

    badge = _rich_badge(row.kind, row.tone)
    title = escape(row.title)
    second = "  ".join(
        escape(p) for p in [row.meta, row.subtitle] if p
    )
    if second:
        return f"{badge}  {title}\n[dim]{second}[/dim]"
    return f"{badge}  {title}"


def render_detail(row: SelectionRow, *, compact: bool = False, width: int | None = None) -> str:
    if row.kind == "section":
        return ""
    if compact:
        return _compact_detail(row, width=width)
    return _rich_detail(row)


def _rich_badge(kind: str, tone: str) -> str:
    color = _TONE_COLOR.get(tone)
    label = _KIND_LABEL.get(kind, kind.upper())
    if color:
        return f"[bold {color}]● {label}[/bold {color}]"
    return f"[dim]●[/dim] [bold]{label}[/bold]"


def _rich_detail(row: SelectionRow) -> str:
    parts: list[str] = []

    title = escape(row.title)
    label = escape(row.label)
    status = escape(row.status or row.meta)

    color = _TONE_COLOR.get(row.tone)
    if color:
        parts.append(f"[bold {color}]{title}[/bold {color}]")
    else:
        parts.append(f"[bold]{title}[/bold]")

    tag = "  ".join(p for p in [label, status] if p)
    if tag:
        parts.append(f"[dim]{tag}[/dim]")

    if row.subtitle:
        parts.append("")
        parts.append("[dim]── notes[/dim]")
        parts.append(escape(row.subtitle))

    actions = row.actions or []
    if actions:
        parts.append("")
        parts.append("[dim]── commands[/dim]")
        for action in actions:
            parts.append(f"[dim]$[/dim] {escape(action)}")
    elif row.detail and row.detail != row.title:
        parts.append("")
        parts.append(escape(row.detail))

    return "\n".join(parts)


def _compact_detail(row: SelectionRow, width: int | None = None) -> str:
    detail = structured_detail(row)
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


def usable_compact_row_width(width: int | None) -> int | None:
    if width is None:
        return None
    return max(width - COMPACT_ROW_CHROME_WIDTH, MIN_WRAP_WIDTH)


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
            tone="urgent",
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


def build_flat_rows(
    repos: list[dict],
    tasks: list[dict],
    inbox: list[dict],
    review: list[dict],
) -> list[SelectionRow]:
    sections = build_dashboard_sections(repos, tasks, inbox, review)
    rows: list[SelectionRow] = []
    for section in sections:
        if len(sections) > 1:
            rows.append(_section_header_row(section.title))
        rows.extend(section.rows)
    return rows


def _section_header_row(title: str) -> SelectionRow:
    return SelectionRow(
        kind="section",
        label=title,
        title=title,
        subtitle="",
        meta="",
        detail="",
        tone="muted",
    )


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
