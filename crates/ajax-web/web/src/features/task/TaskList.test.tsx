import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, afterEach } from "vitest";
import { act, render, fireEvent, screen, within, waitFor } from "@testing-library/react";
import * as api from "@/shared/lib/api";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import TaskList from "./TaskList";
import type { BrowserCockpitView } from "@/shared/lib/types";

const stylesSource = readOrderedStylesSource(
  join(dirname(fileURLToPath(import.meta.url)), "../.."),
);

const NOW_SECS = Math.floor(Date.now() / 1000);

function swipeRowLeft(row: HTMLElement, distance = 200) {
  fireEvent.touchStart(row, { touches: [{ clientX: 320, clientY: 40 }] });
  fireEvent.touchMove(row, { touches: [{ clientX: 320 - distance, clientY: 40 }] });
  fireEvent.touchEnd(row, { changedTouches: [{ clientX: 320 - distance, clientY: 40 }] });
}

function swipeWrapRight(wrap: HTMLElement, distance = 200) {
  fireEvent.touchStart(wrap, { touches: [{ clientX: 120, clientY: 40 }] });
  fireEvent.touchMove(wrap, { touches: [{ clientX: 120 + distance, clientY: 40 }] });
  fireEvent.touchEnd(wrap, { changedTouches: [{ clientX: 120 + distance, clientY: 40 }] });
}

const cockpit: BrowserCockpitView = {
  backend: { authority: "host-native", control_enabled: true },
  repos: {
    repos: [
      { name: "web", attention_items: 2 },
      { name: "api", attention_items: 0 },
    ],
  },
  inbox: { items: [] },
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

  it("shows the non-destructive visible action inline on the resting row", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    const row = screen.getByRole("button", { name: /web\/a/ });
    expect(wrap).not.toHaveClass("is-revealed-wrap");
    expect(within(wrap).getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
    expect(screen.queryByText("Open")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
  });

  it("exposes Drop after a left swipe even when non-drop actions precede it", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    const row = screen.getByRole("button", { name: /web\/a/ });
    swipeRowLeft(row);
    expect(wrap).toHaveClass("is-revealed-wrap");
    expect(row).toHaveClass("is-revealed");
    expect(within(wrap).getByRole("button", { name: "Drop" })).toBeInTheDocument();
    expect(within(wrap).getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
  });

  it("closes a revealed row on swipe right", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    const row = screen.getByRole("button", { name: /web\/a/ });
    swipeRowLeft(row);
    expect(wrap).toHaveClass("is-revealed-wrap");

    swipeWrapRight(wrap);
    expect(wrap).not.toHaveClass("is-revealed-wrap");
    expect(row).not.toHaveClass("is-revealed");
    expect(row.style.transform).toBe("translateX(-0px)");
  });

  it("closes the previously revealed row when another row is revealed", () => {
    const twoReveal: BrowserCockpitView = {
      ...cockpit,
      cards: [
        cockpit.cards[0],
        {
          ...cockpit.cards[1],
          actions: [
            {
              action: "drop",
              label: "Drop",
              destructive: true,
              confirmation_required: true,
            },
          ],
        },
        cockpit.cards[2],
      ],
    };
    render(<TaskList cockpit={twoReveal} />);
    const wrapA = screen.getByTestId("task-row-wrap-web/a");
    const rowA = screen.getByRole("button", { name: /web\/a/ });
    const wrapB = screen.getByTestId("task-row-wrap-web/b");
    const rowB = screen.getByRole("button", { name: /web\/b/ });

    swipeRowLeft(rowA);
    expect(wrapA).toHaveClass("is-revealed-wrap");

    swipeRowLeft(rowB);
    expect(wrapA).not.toHaveClass("is-revealed-wrap");
    expect(wrapB).toHaveClass("is-revealed-wrap");
  });

  it("auto-hides a settled reveal after ten seconds", () => {
    vi.useFakeTimers();
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    const row = screen.getByRole("button", { name: /web\/a/ });
    swipeRowLeft(row);
    expect(wrap).toHaveClass("is-revealed-wrap");

    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(wrap).not.toHaveClass("is-revealed-wrap");
    expect(row).not.toHaveClass("is-revealed");
  });

  it("does not auto-hide while drop confirm is pending for that row", () => {
    vi.useFakeTimers();
    const dropOnly: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/drop",
          qualified_handle: "web/drop",
          repo: "web",
          title: "Drop me",
          status: "error",
          last_activity_unix_secs: NOW_SECS,
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
    const onResult = vi.fn();
    render(<TaskList cockpit={dropOnly} onResult={onResult} />);
    const wrap = screen.getByTestId("task-row-wrap-web/drop");
    const row = screen.getByRole("button", { name: /web\/drop/ });
    swipeRowLeft(row);

    fireEvent.click(within(wrap).getByRole("button", { name: "Drop" }));
    expect(onResult).toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(wrap).toHaveClass("is-revealed-wrap");
  });

  it("renders every card in one unified list — no Active/Idle split", () => {
    render(<TaskList cockpit={cockpit} />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    expect(within(tasks).queryByRole("group")).toBeNull();
    expect(within(tasks).queryByText("Active")).toBeNull();
    expect(within(tasks).getByRole("button", { name: /web\/a/ })).toBeInTheDocument();
    expect(within(tasks).getByRole("button", { name: /web\/b/ })).toBeInTheDocument();
    expect(within(tasks).getByRole("button", { name: /api\/c/ })).toBeInTheDocument();
  });

  it("renders no project filter pill bar", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.queryByRole("navigation")).toBeNull();
    expect(screen.queryByRole("button", { name: "All" })).toBeNull();
  });

  it("shows the concrete status_explanation on active rows (Option A)", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowA = screen.getByRole("button", { name: /web\/a/ });
    expect(within(rowA).getByText("CI failed")).toHaveClass("task-row-status");
    expect(rowA).toHaveClass("tone-error");
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(within(rowB).getByText("Agent working")).toHaveClass("task-row-status");
    expect(rowB).toHaveClass("tone-running");
  });

  it("falls back to Working/Waiting/Error labels and hides the label for idle rows", () => {
    const noExplain: BrowserCockpitView = {
      ...cockpit,
      cards: [
        { id: "web/x", qualified_handle: "web/x", repo: "web", title: "X", status: "running", last_activity_unix_secs: 0, actions: [] },
        { id: "api/w", qualified_handle: "api/w", repo: "api", title: "W", status: "waiting", last_activity_unix_secs: 0, actions: [] },
        { id: "api/e", qualified_handle: "api/e", repo: "api", title: "E", status: "error", last_activity_unix_secs: 0, actions: [] },
        cockpit.cards[2],
      ],
    };
    render(<TaskList cockpit={noExplain} />);
    expect(within(screen.getByRole("button", { name: /web\/x/ })).getByText("Working")).toHaveClass("task-row-status");
    expect(within(screen.getByRole("button", { name: /api\/w/ })).getByText("Waiting")).toHaveClass("task-row-status");
    expect(within(screen.getByRole("button", { name: /api\/e/ })).getByText("Error")).toHaveClass("task-row-status");
    const rowC = screen.getByRole("button", { name: /api\/c/ });
    expect(rowC).not.toHaveTextContent(/Working|Waiting|Error|Ready for review/);
  });

  it("labels review-attention idle rows Ready for review with the done tone", () => {
    const review: BrowserCockpitView = {
      ...cockpit,
      cards: [
        { id: "web/r", qualified_handle: "web/r", repo: "web", title: "R", status: "idle", attention: "review", last_activity_unix_secs: 0, actions: [] },
      ],
    };
    render(<TaskList cockpit={review} />);
    const row = screen.getByRole("button", { name: /web\/r/ });
    expect(within(row).getByText("Ready for review")).toHaveClass("task-row-status");
    expect(row).toHaveClass("tone-done");
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

  it("shows the card's non-destructive visible actions inline and never shows Drop inline", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    expect(within(wrap).getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Drop" })).not.toBeInTheDocument();
  });

  it("runs the inline action without opening the task", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(
      within(screen.getByTestId("task-row-wrap-web/a")).getByRole("button", { name: "Fix CI" }),
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(onOpenTask).not.toHaveBeenCalled();
    spy.mockRestore();
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
    const wrap = screen.getByTestId("task-row-wrap-web/b");
    const webBRow = screen.getByRole("button", { name: /web\/b/ });
    expect(wrap).not.toHaveClass("is-revealed-wrap");
    expect(within(webBRow).getByRole("button", { name: "Review" })).toBeInTheDocument();
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
    swipeRowLeft(row);
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

  it("styles the row status as clean neutral text, carrying tone only for waiting and error", () => {
    const statusRule = stylesSource.match(/\.task-row-status\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(statusRule).toMatch(/font-size:\s*var\(--text-label\)/);
    expect(statusRule).toMatch(/font-weight:\s*500/);
    expect(statusRule).toMatch(/color:\s*var\(--ink-muted\)/);
    expect(statusRule).not.toMatch(/var\(--tone/);
    expect(statusRule).not.toMatch(/text-transform/);

    const toneStatusRule =
      stylesSource.match(/\.task-row\.tone-waiting \.task-row-status,\s*\.task-row\.tone-error \.task-row-status\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(toneStatusRule).toMatch(/color:\s*var\(--tone\)/);

    const listRule = stylesSource.match(/\.task-list\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(listRule).toMatch(/border-radius:\s*var\(--radius\)/);
    expect(listRule).not.toMatch(/box-shadow/);

    const idleDotRule = stylesSource.match(/\.status-dot\.tone-idle\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(idleDotRule).toMatch(/background:\s*transparent/);
  });

  it("does not mark the resting row as revealed", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.getByTestId("task-row-wrap-web/a")).not.toHaveClass("is-revealed-wrap");
  });

  it("opens the task when the resting row is tapped instead of surfacing actions", () => {
    const onOpenTask = vi.fn();
    render(<TaskList cockpit={cockpit} onOpenTask={onOpenTask} />);
    fireEvent.click(screen.getByRole("button", { name: /web\/a/ }));
    expect(onOpenTask).toHaveBeenCalledWith("web/a");
  });

  it("marks rows with swipe reveal and sets the reveal width CSS var", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    expect(wrap).toHaveClass("has-reveal");
    expect(wrap).toHaveStyle({ "--task-row-reveal-width": "158px" });
    expect(screen.getByTestId("task-row-wrap-api/c")).not.toHaveClass("has-reveal");
  });

  it("positions reveal behind the row and keeps revealed row taps on the action (#1038)", () => {
    const revealRule =
      stylesSource.match(/\.task-row-reveal\s*\{([^}]*)\}/)?.[1] ?? "";
    const restingRevealRule =
      stylesSource.match(
        /\.task-row-wrap\.has-reveal:not\(\.is-revealed-wrap\) \.task-row-reveal\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    const revealedRule =
      stylesSource.match(/\.task-row\.is-revealed\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(revealRule).toMatch(/position:\s*absolute/);
    expect(revealRule).toMatch(/width:\s*var\(--task-row-reveal-width/);
    expect(restingRevealRule).toMatch(/visibility:\s*hidden/);
    expect(restingRevealRule).toMatch(/pointer-events:\s*none/);
    expect(stylesSource).not.toMatch(
      /\.task-row-wrap\.has-reveal\s*\{[^}]*grid-template-columns/,
    );
    expect(revealedRule).toMatch(/pointer-events:\s*none/);
  });

  it("keeps Create PR and Merge PR off the dashboard row while other actions stay inline", () => {
    const prCard: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/pr",
          qualified_handle: "web/pr",
          repo: "web",
          title: "PR",
          status: "idle",
          last_activity_unix_secs: 0,
          actions: [
            { action: "create-pr", label: "Create PR", destructive: false, confirmation_required: false },
            { action: "merge-pr", label: "Merge PR", destructive: false, confirmation_required: false },
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
          ],
        },
      ],
    };
    render(<TaskList cockpit={prCard} />);
    const row = screen.getByRole("button", { name: /web\/pr/ });
    expect(within(row).getByRole("button", { name: "Review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create PR" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Merge PR" })).not.toBeInTheDocument();
  });

  it("drops the row transform transition only while a touch drag is held", () => {
    render(<TaskList cockpit={cockpit} />);
    const wrap = screen.getByTestId("task-row-wrap-web/a");
    const row = within(wrap).getByRole("button", { name: /A/ });

    fireEvent.touchStart(row, { touches: [{ clientX: 320, clientY: 40 }] });
    fireEvent.touchMove(row, { touches: [{ clientX: 300, clientY: 40 }] });
    expect(row.style.transition).toBe("background 120ms var(--ease)");
    fireEvent.touchEnd(row, { changedTouches: [{ clientX: 300, clientY: 40 }] });
    expect(row.style.transition).toBe("");

    const rowRule = stylesSource.match(/\.task-row\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(rowRule).toMatch(/transform 220ms/);
  });
});
