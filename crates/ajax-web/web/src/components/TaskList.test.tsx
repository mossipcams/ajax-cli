import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen, within } from "@testing-library/react";
import TaskList from "./TaskList";
import type { BrowserCockpitView } from "../types";

const stylesSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../styles.css"),
  "utf8",
);

const NOW_SECS = Math.floor(Date.now() / 1000);

const cockpit: BrowserCockpitView = {
  backend: { authority: "host-native", control_enabled: true },
  repos: {
    repos: [
      { name: "web", attention_items: 2 },
      { name: "api", attention_items: 0 },
    ],
  },
  cards: [
    {
      id: "web/a",
      qualified_handle: "web/a",
      repo: "web",
      title: "A",
      status: "error",
      status_explanation: "CI failed",
      last_activity_unix_secs: NOW_SECS - 60,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
        { action: "fix-ci", label: "Fix CI", destructive: false, confirmation_required: false },
        { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
      ],
    },
    {
      id: "web/b",
      qualified_handle: "web/b",
      repo: "web",
      title: "B",
      status: "running",
      status_explanation: "Agent working",
      last_activity_unix_secs: NOW_SECS - 300,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
      ],
    },
    {
      id: "api/c",
      qualified_handle: "api/c",
      repo: "api",
      title: "C",
      status: "idle",
      last_activity_unix_secs: 0,
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/a", severity: 1 }] },
};

describe("TaskList", () => {
  it("shows relative last-activity time on task rows and omits it when unset", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(rowB).toHaveTextContent("5m ago");
    const rowC = screen.getByRole("button", { name: /api\/c/ });
    expect(rowC).not.toHaveTextContent("ago");
  });

  it("renders the inbox item as a compact row with explanation and a swipe-revealed action", () => {
    const { container } = render(<TaskList cockpit={cockpit} />);
    const webARow = screen.getByRole("button", { name: /web\/a/ });
    expect(webARow).toHaveClass("task-row");
    expect(webARow).toHaveClass("is-inbox");
    expect(webARow).toHaveAttribute("data-handle", "web/a");
    expect(screen.getByText("CI failed")).toBeInTheDocument();
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/a']");
    expect(wrap!.querySelector(".task-row-reveal")).not.toBeNull();
    expect(screen.getByText("Fix CI")).toBeInTheDocument();
    expect(screen.queryByText("Open")).not.toBeInTheDocument();
    expect(screen.queryByText("Resume")).not.toBeInTheDocument();
  });

  it("renders calm task rows excluding inbox tasks", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.getByRole("button", { name: /web\/b/ })).toHaveAttribute("data-handle", "web/b");
    expect(screen.getByRole("button", { name: /api\/c/ })).toHaveAttribute("data-handle", "api/c");
    const webARow = screen.getByRole("button", { name: /web\/a/ });
    expect(webARow).toHaveClass("is-inbox");
    expect(webARow).toHaveAttribute("data-handle", "web/a");
  });

  it("groups attention tasks under Needs you and keeps them out of Tasks", () => {
    render(<TaskList cockpit={cockpit} />);
    const needsYou = screen.getByRole("region", { name: "Needs you" });
    const tasks = screen.getByRole("region", { name: "Tasks" });
    // web/a is an inbox item: it belongs to Needs you and must not be duplicated
    // into the calm Tasks group.
    expect(within(needsYou).getByRole("button", { name: /web\/a/ })).toBeInTheDocument();
    expect(within(tasks).queryByRole("button", { name: /web\/a/ })).toBeNull();
    // Calm rows live only under Tasks.
    expect(within(tasks).getByRole("button", { name: /web\/b/ })).toBeInTheDocument();
    expect(within(needsYou).queryByRole("button", { name: /web\/b/ })).toBeNull();
  });

  it("shows per-repo attention counts on project pills", () => {
    render(<TaskList cockpit={cockpit} />);
    const webPill = screen.getByRole("button", { name: "web — 2 need attention" });
    expect(webPill).toHaveAttribute("aria-label", "web — 2 need attention");
    expect(within(webPill).getByText("2")).toHaveClass("pill-badge");
    const apiPill = screen.getByRole("button", { name: "api" });
    expect(apiPill).toHaveAttribute("aria-label", "api");
  });

  it("marks the active project pill for assistive tech", () => {
    render(<TaskList cockpit={cockpit} selectedProject="api" />);
    const allPill = screen.getByRole("button", { name: "All" });
    const apiPill = screen.getByRole("button", { name: "api" });
    expect(apiPill).toHaveAttribute("aria-current", "true");
    expect(allPill).not.toHaveAttribute("aria-current");
  });

  it("offers project pills and reports selection", () => {
    const onSelectProject = vi.fn();
    render(<TaskList cockpit={cockpit} onSelectProject={onSelectProject} />);
    expect(screen.getByRole("button", { name: "All" })).toBeInTheDocument();
    const webPill = screen.getByRole("button", { name: "web — 2 need attention" });
    fireEvent.click(webPill);
    expect(onSelectProject).toHaveBeenCalledWith("web");
  });

  it("filters by the selected project", () => {
    render(<TaskList cockpit={cockpit} selectedProject="api" />);
    expect(screen.getByRole("button", { name: /api\/c/ })).toHaveAttribute("data-handle", "api/c");
    expect(screen.queryByRole("button", { name: /web\/b/ })).not.toBeInTheDocument();
  });

  it("empty state points at the new-task CTA", () => {
    const docsCockpit: BrowserCockpitView = {
      ...cockpit,
      repos: { repos: [...cockpit.repos.repos, { name: "docs" }] },
    };
    render(<TaskList cockpit={docsCockpit} selectedProject="docs" />);
    expect(screen.getByText("No tasks in docs yet — start one below.")).toBeInTheDocument();

    const emptyCockpit: BrowserCockpitView = {
      ...cockpit,
      cards: [],
      inbox: { items: [] },
    };
    render(<TaskList cockpit={emptyCockpit} />);
    expect(screen.getByText("All quiet — start a new task below.")).toBeInTheDocument();
  });

  it("opens a task when a row is tapped", () => {
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(screen.getByRole("button", { name: /api\/c/ }));
    expect(onOpenTask).toHaveBeenCalledWith("api/c");
  });

  it("opens an inbox task when the row is tapped", () => {
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(screen.getByRole("button", { name: /web\/a/ }));
    expect(onOpenTask).toHaveBeenCalledWith("web/a");
  });

  it("does not reveal resume as a calm-row action", () => {
    const { container } = render(<TaskList cockpit={cockpit} />);
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/b']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).toBeNull();
  });

  it("reveals a swipe action behind a calm row that has actions", () => {
    const withAction: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/b",
          qualified_handle: "web/b",
          repo: "web",
          title: "B",
          status: "idle",
          last_activity_unix_secs: 0,
          actions: [
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
          ],
        },
      ],
      inbox: { items: [] },
    };
    const { container } = render(<TaskList cockpit={withAction} />);
    const webBRow = screen.getByRole("button", { name: /web\/b/ });
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/b']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).not.toBeNull();
    expect(webBRow).toHaveAttribute("data-handle", "web/b");
  });

  it("renders no reveal for a calm row without non-resume actions", () => {
    const { container } = render(<TaskList cockpit={cockpit} />);
    const wrap = container.querySelector(".task-row-wrap[data-handle='api/c']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).toBeNull();
  });

  it("keeps is-inbox semantics without a left border stripe", () => {
    const inboxRule =
      stylesSource.match(/\.task-row\.is-inbox\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(inboxRule).not.toMatch(/border-left/);
    expect(inboxRule).not.toMatch(/padding-left:\s*calc/);
  });

  it("uses accent for the active project pill and warn for attention badges", () => {
    const activePillRule =
      stylesSource.match(/\.project-pill\.is-active\s*\{([^}]*)\}/)?.[1] ?? "";
    const pillBadgeRule = stylesSource.match(/\.pill-badge\s*\{([^}]*)\}/)?.[1] ?? "";
    const attentionTitleRule =
      stylesSource.match(/\.section-head\.attention \.section-head-title\s*\{([^}]*)\}/)?.[1] ??
      "";
    const attentionCountRule =
      stylesSource.match(/\.section-head\.attention \.section-head-count\s*\{([^}]*)\}/)?.[1] ??
      "";

    expect(activePillRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
    expect(activePillRule).not.toMatch(/var\(--warn/);
    expect(pillBadgeRule).toMatch(/var\(--warn/);
    expect(attentionTitleRule).toMatch(/var\(--warn/);
    expect(attentionCountRule).toMatch(/var\(--warn/);
  });
});