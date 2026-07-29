import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen, within } from "@testing-library/react";
import TaskList from "./TaskList";
import type { BrowserCockpitView } from "@/shared/lib/types";

const stylesSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"),
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
      attention: "needs-you",
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
      attention: "active",
      status_explanation: "Agent working",
      last_activity_unix_secs: NOW_SECS - 300,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
      ],
    },
    {
      id: "web/r",
      qualified_handle: "web/r",
      repo: "web",
      title: "R",
      status: "idle",
      attention: "review",
      last_activity_unix_secs: NOW_SECS - 120,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
        { action: "ship", label: "Ship", destructive: false, confirmation_required: false },
        { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
      ],
    },
    {
      id: "api/c",
      qualified_handle: "api/c",
      repo: "api",
      title: "C",
      status: "idle",
      attention: "idle",
      last_activity_unix_secs: 0,
      actions: [],
    },
  ],
};

describe("TaskList", () => {
  it("shows relative last-activity time on task rows and omits it when unset", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(rowB).toHaveTextContent("5m ago");
    const rowC = screen.getByRole("button", { name: /api\/c/ });
    expect(rowC).not.toHaveTextContent("ago");
  });

  it("renders every card as a calm row with attention-band grouping", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.getByText("Needs you", { selector: ".task-band-label" })).toBeInTheDocument();
    const webARow = screen.getByRole("button", { name: /web\/a/ });
    expect(webARow).not.toHaveClass("is-inbox");
    expect(webARow).not.toHaveClass("is-next");
    expect(webARow).toHaveAttribute("data-handle", "web/a");
    expect(screen.getByText("CI failed")).toBeInTheDocument();
  });

  it("puts the first non-destructive action inline and secondary actions in swipe reveal", () => {
    render(<TaskList cockpit={cockpit} />);
    // web/a: fix-ci is inline; drop is excluded; no second fix-ci in reveal.
    expect(screen.getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Drop" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
  });

  it("groups tasks by attention band with idle in the disclosure", () => {
    render(<TaskList cockpit={cockpit} />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    const label = { selector: ".task-band-label" } as const;
    expect(within(tasks).getByText("Needs you", label)).toHaveClass("task-band-label");
    expect(within(tasks).getByText("Active", label)).toHaveClass("task-band-label");
    const tierOrder = within(tasks)
      .getAllByText(/^(Needs you|Ready to review|Active)$/, label)
      .map((node) => node.textContent);
    expect(tierOrder).toEqual(["Needs you", "Ready to review", "Active"]);

    const idle = within(tasks).getByRole("group");
    expect(idle).toHaveAttribute("open");
    expect(within(idle).getByRole("button", { name: /api\/c/ })).toBeInTheDocument();
    expect(within(idle).queryByRole("button", { name: /web\/a/ })).toBeNull();
    expect(within(idle).queryByRole("button", { name: /web\/b/ })).toBeNull();
  });

  it("leads with the task list — no fleet-summary bar above it", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.queryByRole("group", { name: "Fleet status" })).toBeNull();
  });

  it("flags a running task that has gone quiet past the threshold", () => {
    // web/b last activity is 5m ago — past the 4m quiet boundary.
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(within(rowB).getByText(/Quiet 5m — no output/)).toBeInTheDocument();
  });

  it("marks faulted repos with a fault dot on the project pill", () => {
    render(<TaskList cockpit={cockpit} />);
    // The accessible label is the fault contract; the dot itself is decorative.
    const webPill = screen.getByRole("button", { name: "web — has a fault" });
    expect(webPill).toHaveAttribute("aria-label", "web — has a fault");
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
    const webPill = screen.getByRole("button", { name: "web — has a fault" });
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

  it("shows an inline control for a row that has one non-destructive action", () => {
    const withAction: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/b",
          qualified_handle: "web/b",
          repo: "web",
          title: "B",
          status: "idle",
          attention: "review",
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

  it("renders no control at all for a row with nothing to run", () => {
    const onlyIdle: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "api/c",
          qualified_handle: "api/c",
          repo: "api",
          title: "C",
          status: "idle",
          attention: "idle",
          last_activity_unix_secs: 0,
          actions: [],
        },
      ],
    };
    render(<TaskList cockpit={onlyIdle} />);
    expect(screen.getByRole("button", { name: /api\/c/ })).toHaveAttribute("data-handle", "api/c");
    // The row tap and its chevron already open the task; a button repeating
    // that would be a third affordance for one action.
    expect(screen.queryByRole("button", { name: "Open" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Answer" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Review" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Fix CI" })).not.toBeInTheDocument();
  });

  it("renders the human-readable title as the row's primary line", () => {
    render(<TaskList cockpit={cockpit} />);
    const rowB = screen.getByRole("button", { name: /web\/b/ });
    expect(within(rowB).getByText("B")).toHaveClass("task-row-title");
    expect(within(rowB).getByText("web/b")).toHaveClass("task-row-handle");
  });

  it("uses accent for the active project pill and danger for the fault dot", () => {
    const activePillRule =
      stylesSource.match(/\.project-pill\.is-active\s*\{([^}]*)\}/)?.[1] ?? "";
    const faultDotRule = stylesSource.match(/\.pill-fault-dot\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(activePillRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
    expect(activePillRule).not.toMatch(/var\(--warn/);
    expect(faultDotRule).toMatch(/var\(--danger\)/);
  });

  it("keeps swipe-reveal action labels on one line with enough horizontal pad", () => {
    expect(stylesSource).toMatch(/\.task-row-reveal\s+\.action[\s\S]*?white-space:\s*nowrap/);
    expect(stylesSource).toMatch(
      /\.task-row-reveal\s+\.action[\s\S]*?padding:\s*[0-9]+px\s+(?:1[2-9]|[2-9]\d)px/,
    );
  });

  it("groups_render_in_operator_order", () => {
    render(<TaskList cockpit={cockpit} />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    const label = { selector: ".task-band-label" } as const;
    const headings = within(tasks)
      .getAllByText(/^(Needs you|Ready to review|Active|Idle)$/, label)
      .map((node) => node.textContent);
    expect(headings).toEqual(["Needs you", "Ready to review", "Active", "Idle"]);
  });

  it("group_membership_comes_from_attention_not_status", () => {
    const reviewable: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/x",
          qualified_handle: "web/x",
          repo: "web",
          title: "X",
          status: "idle",
          attention: "review",
          last_activity_unix_secs: 0,
          actions: [
            { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
            { action: "ship", label: "Ship", destructive: false, confirmation_required: false },
          ],
        },
      ],
    };
    render(<TaskList cockpit={reviewable} />);
    expect(screen.getByText("Ready to review", { selector: ".task-band-label" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /web\/x/ })).toBeInTheDocument();
    expect(screen.queryByText("Idle", { selector: ".task-band-label" })).toBeNull();
  });

  it("waiting_row_never_offers_drop", () => {
    const waiting: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/w",
          qualified_handle: "web/w",
          repo: "web",
          title: "W",
          status: "waiting",
          attention: "needs-you",
          last_activity_unix_secs: NOW_SECS - 30,
          actions: [
            { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
            { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
          ],
        },
      ],
    };
    render(<TaskList cockpit={waiting} />);
    // Drop is never the offered next step on a task that needs an answer, and
    // answering happens in the terminal — so the row carries no control, just
    // its own tap. The status line says what it is waiting on.
    expect(screen.queryByRole("button", { name: "Drop" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Answer" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Resume" })).toBeNull();
  });

  it("review_row_offers_ship_inline", () => {
    render(<TaskList cockpit={cockpit} />);
    const ship = screen.getByRole("button", { name: "Ship" });
    expect(ship).toHaveAttribute("data-action", "ship");
    expect(ship).toBeVisible();
  });

  it("inline_control_is_never_destructive", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.queryByRole("button", { name: "Drop" })).toBeNull();
  });

  it("swipe_reveal_excludes_the_inline_control", () => {
    render(<TaskList cockpit={cockpit} />);
    expect(screen.getAllByRole("button", { name: "Fix CI" })).toHaveLength(1);
  });
});
