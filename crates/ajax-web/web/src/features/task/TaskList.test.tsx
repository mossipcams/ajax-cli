import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, afterEach } from "vitest";
import { act, render, fireEvent, screen, within } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import TaskList from "./TaskList";
import type { BrowserCockpitView } from "@/shared/lib/types";

const stylesSource = readOrderedStylesSource(
  join(dirname(fileURLToPath(import.meta.url)), "../.."),
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
};

describe("TaskList", () => {
  afterEach(() => {
    vi.useRealTimers();
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
  });

  it("pauses the relative-time ticker while the document is hidden", () => {
    vi.useFakeTimers();
    const baseSecs = Math.floor(Date.now() / 1000);
    vi.setSystemTime(baseSecs * 1000);

    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(rowB).toHaveTextContent("5m ago");

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });

    act(() => {
      vi.advanceTimersByTime(65 * 60_000);
    });
    expect(rowB).toHaveTextContent("5m ago");

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(rowB).toHaveTextContent("1h ago");
  });

  it("shows relative last-activity time on task rows and omits it when unset", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(rowB).toHaveTextContent("5m ago");
    const rowC = screen.getByRole("button", { name: /api\/c/ });
    expect(rowC).not.toHaveTextContent("ago");
  });

  it("renders every card as a calm row — no inbox section, no inline action", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.queryByRole("region", { name: "Needs you" })).toBeNull();
    const webARow = screen.getByRole("button", { name: /web\/a/ });
    expect(webARow).toHaveClass("task-row");
    expect(webARow).not.toHaveClass("is-inbox");
    expect(webARow).not.toHaveClass("is-next");
    expect(webARow).toHaveAttribute("data-handle", "web/a");
    expect(screen.getByText("CI failed")).toBeInTheDocument();
  });

  it("reveals the first non-resume action behind a row via swipe", () => {
    render(<TaskList cockpit={cockpit} />);
    // web/a: resume is filtered, so Fix CI is the reveal; Drop stays on detail.
    expect(screen.getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
    expect(screen.queryByText("Open")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
  });

  it("places running/error tasks in Active and idle tasks in the disclosure", () => {
    render(<TaskList cockpit={cockpit} />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    expect(within(tasks).getByText("Active")).toHaveClass("task-band-label");

    const idle = within(tasks).getByRole("group");
    expect(idle).toHaveAttribute("open");
    // web/a (error) and web/b (running) are active; only api/c is idle.
    expect(within(idle).getByRole("button", { name: /api\/c/ })).toBeInTheDocument();
    expect(within(idle).queryByRole("button", { name: /web\/a/ })).toBeNull();
    expect(within(idle).queryByRole("button", { name: /web\/b/ })).toBeNull();
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

    const emptyCockpit: BrowserCockpitView = { ...cockpit, cards: [] };
    render(<TaskList cockpit={emptyCockpit} />);
    expect(screen.getByText("All quiet — start a new task below.")).toBeInTheDocument();
  });

  it("opens a task when a row is tapped", () => {
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(screen.getByRole("button", { name: /api\/c/ }));
    expect(onOpenTask).toHaveBeenCalledWith("api/c");
  });

  it("does not reveal resume as a row action", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
  });

  it("reveals a swipe action behind a row that has actions", () => {
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
    };
    render(<TaskList cockpit={withAction} />);
    const webBRow = screen.getByRole("button", { name: /web\/b/ });
    expect(screen.getByRole("button", { name: "Review" })).toBeInTheDocument();
    expect(webBRow).toHaveAttribute("data-handle", "web/b");
  });

  it("renders no reveal for a row without non-resume actions", () => {
    const onlyIdle: BrowserCockpitView = {
      ...cockpit,
      cards: [
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
    };
    render(<TaskList cockpit={onlyIdle} />);
    expect(screen.getByRole("button", { name: /api\/c/ })).toHaveAttribute("data-handle", "api/c");
    expect(screen.queryByRole("button", { name: "Review" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Fix CI" })).not.toBeInTheDocument();
  });

  it("renders the human-readable title as the row's primary line", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(within(rowB).getByText("B")).toHaveClass("task-row-title");
    expect(within(rowB).getByText("web/b")).toHaveClass("task-row-handle");
  });

  it("revealed Drop receives taps instead of the full-row opener (#1038)", () => {
    const dropOnly: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/drop",
          qualified_handle: "web/drop",
          repo: "web",
          title: "Drop me",
          status: "idle",
          last_activity_unix_secs: 0,
          actions: [
            {
              action: "drop",
              label: "Drop",
              destructive: true,
              confirmation_required: true,
            },
          ],
        },
      ],
    };
    const onOpenTask = vi.fn();
    const onResult = vi.fn();
    render(
      <TaskList cockpit={dropOnly} onOpenTask={onOpenTask} onResult={onResult} />,
    );
    const row = screen.getByRole("button", { name: /web\/drop/ });
    fireEvent.touchStart(row, { touches: [{ clientX: 320, clientY: 40 }] });
    fireEvent.touchMove(row, { touches: [{ clientX: 120, clientY: 40 }] });
    fireEvent.touchEnd(row, { changedTouches: [{ clientX: 120, clientY: 40 }] });
    expect(row).toHaveClass("is-revealed");

    fireEvent.click(screen.getByRole("button", { name: "Drop" }));
    expect(onResult).toHaveBeenCalledWith(
      "Confirm Drop for web/drop?",
      null,
      false,
      expect.objectContaining({
        pendingConfirm: expect.objectContaining({ action: expect.objectContaining({ action: "drop" }) }),
      }),
    );
    expect(onOpenTask).not.toHaveBeenCalled();
  });

  it("uses accent for the active project pill and warn for attention badges", () => {
    const activePillRule =
      stylesSource.match(/\.project-pill\.is-active\s*\{([^}]*)\}/)?.[1] ?? "";
    const pillBadgeRule = stylesSource.match(/\.pill-badge\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(activePillRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
    expect(activePillRule).not.toMatch(/var\(--warn/);
    expect(pillBadgeRule).toMatch(/var\(--warn/);
  });

  it("Fix CI is tappable without swipe when the reveal strip is reserved (#1122)", () => {
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(screen.getByRole("button", { name: "Fix CI" }));
    expect(onOpenTask).not.toHaveBeenCalled();
  });

  it("marks rows with a reserved reveal strip (#1122)", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    expect(wrap).toHaveClass("has-reveal");
    expect(wrap).toHaveStyle({ "--task-row-reveal-width": "158px" });
    expect(screen.getByTestId("task-row-wrap-api/c")).not.toHaveClass("has-reveal");
  });

  it("reserves reveal width and keeps revealed row taps on the action (#1122, #1038)", () => {
    const wrapRule =
      stylesSource.match(/\.task-row-wrap\.has-reveal\s*\{([^}]*)\}/)?.[1] ?? "";
    const revealRule =
      stylesSource.match(/\.task-row-wrap\.has-reveal \.task-row-reveal\s*\{([^}]*)\}/)?.[1] ??
      "";
    const revealedRule =
      stylesSource.match(/\.task-row\.is-revealed\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(wrapRule).toMatch(/grid-template-columns:[^;]*var\(--task-row-reveal-width/);
    expect(revealRule).toMatch(/z-index:\s*2/);
    expect(revealedRule).toMatch(/pointer-events:\s*none/);
  });
});
