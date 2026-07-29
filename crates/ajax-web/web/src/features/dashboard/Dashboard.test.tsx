import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen, within } from "@testing-library/react";
import Dashboard from "./Dashboard";
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
        { action: "repair", label: "Repair", destructive: false, confirmation_required: false },
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
        { action: "review", label: "Review", destructive: false, confirmation_required: false },
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

const rowFor = (handle: string) => screen.getByRole("button", { name: new RegExp(handle) });
const rowEl = (handle: string) => screen.getByTestId(`task-row-${handle}`);

describe("Dashboard", () => {
  // ---- the point of the rebuild: one tap runs anything a task offers -------

  it("gives every safe action its own button on the row", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const row = rowEl("web/r");
    expect(within(row).getByRole("button", { name: "Review" })).toHaveAttribute(
      "data-action",
      "review",
    );
    expect(within(row).getByRole("button", { name: "Ship" })).toHaveAttribute(
      "data-action",
      "ship",
    );
  });

  it("hides nothing behind a gesture — a multi-action row shows all of them", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const dispatched = within(rowEl("web/a"))
      .getAllByRole("button")
      .map((button) => button.getAttribute("data-action"))
      .filter(Boolean);
    expect(dispatched).toEqual(["fix-ci", "repair"]);
  });

  it("never offers Drop on the dashboard", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(screen.queryByRole("button", { name: "Drop" })).toBeNull();
    const destructive = screen
      .getAllByRole("button")
      .filter((button) => button.dataset.destructive === "true");
    expect(destructive).toEqual([]);
  });

  it("never offers Resume — opening the task already dispatches it", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(screen.queryByRole("button", { name: "Resume" })).toBeNull();
  });

  it("primary action is full-width on the row", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const actions = within(rowEl("web/a")).getByTestId("task-row-actions");
    const layout = within(actions).getByTestId("action-row");
    expect(layout).toHaveAttribute("data-layout", "primary-key");

    const primarySlot = within(actions).getByTestId("action-primary-slot");
    const fixCi = within(primarySlot).getByRole("button", { name: "Fix CI" });
    expect(fixCi).toHaveClass("primary");
    expect(fixCi).toHaveAttribute("data-action", "fix-ci");

    const secondary = within(actions).getByTestId("task-row-actions-secondary");
    const repair = within(secondary).getByRole("button", { name: "Repair" });
    expect(repair).toHaveAttribute("data-action", "repair");
    expect(repair).not.toHaveClass("primary");

    const actionButtons = within(actions)
      .getAllByRole("button")
      .map((button) => button.getAttribute("data-action"));
    expect(actionButtons).toEqual(["fix-ci", "repair"]);
  });

  it("renders no secondary row when only one safe action exists", () => {
    const oneAction: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/solo",
          qualified_handle: "web/solo",
          repo: "web",
          title: "Solo",
          status: "idle",
          attention: "review",
          last_activity_unix_secs: NOW_SECS - 120,
          actions: [
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
            { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
          ],
        },
      ],
    };
    render(<Dashboard cockpit={oneAction} connection="connected" />);
    const actions = within(rowEl("web/solo")).getByTestId("task-row-actions");
    expect(within(actions).queryByTestId("task-row-actions-secondary")).toBeNull();
    expect(within(actions).getByRole("button", { name: "Review" })).toHaveClass("primary");
  });

  it("renders no action line for a row with nothing safe to run", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(within(rowEl("api/c")).queryByTestId("task-row-actions")).toBeNull();
  });

  it("opens the task when the row's text block is tapped", () => {
    const onOpenTask = vi.fn();
    render(<Dashboard cockpit={cockpit} connection="connected" onOpenTask={onOpenTask} />);
    fireEvent.click(rowFor("api/c"));
    expect(onOpenTask).toHaveBeenCalledWith("api/c");
  });

  // ---- server-owned grouping (ported contracts) ----------------------------

  it("groups by attention band in operator order with idle in a disclosure", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const tasks = screen.getByRole("region", { name: "Tasks" });
    const label = { selector: ".task-band-label" } as const;
    const headings = within(tasks)
      .getAllByText(/^(Needs attention|Running now|Ready for action|Recent)$/, label)
      .map((node) => node.textContent);
    expect(headings).toEqual(["Needs attention", "Running now", "Ready for action", "Recent"]);

    const idle = within(tasks).getByRole("group");
    expect(idle).toHaveAttribute("open");
    expect(within(idle).getByRole("button", { name: /api\/c/ })).toBeInTheDocument();
    expect(within(idle).queryByRole("button", { name: /web\/a/ })).toBeNull();
  });

  it("takes group membership from attention, never from status", () => {
    const reviewable: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/x",
          qualified_handle: "web/x",
          repo: "web",
          title: "X",
          // idle status, review band — the band must win.
          status: "idle",
          attention: "review",
          last_activity_unix_secs: 0,
          actions: [
            { action: "ship", label: "Ship", destructive: false, confirmation_required: false },
          ],
        },
      ],
    };
    render(<Dashboard cockpit={reviewable} connection="connected" />);
    expect(
      screen.getByText("Ready for action", { selector: ".task-band-label" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Recent", { selector: ".task-band-label" })).toBeNull();
  });

  // ---- row content --------------------------------------------------------

  it("leads with the title and carries the handle and explanation below it", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const row = rowFor("web/b");
    expect(within(row).getByText("B")).toHaveClass("task-row-title");
    expect(within(row).getByText("web/b")).toHaveClass("task-row-handle");
  });

  it("shows relative last-activity time and omits it when unset", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(rowFor("web/b")).toHaveTextContent("5m ago");
    expect(rowFor("api/c")).not.toHaveTextContent("ago");
  });

  it("flags a running task that has gone quiet past the threshold as stale", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(within(rowFor("web/b")).getByText(/Stale 5m — no output/)).toBeInTheDocument();
  });

  it("always says what a task is doing, falling back to the status word", () => {
    // web/r carries no status_explanation; the row must still read as something.
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    expect(within(rowFor("web/r")).getByText("Idle")).toHaveClass("task-row-note");
    expect(within(rowFor("web/a")).getByText("CI failed")).toHaveClass("task-row-note");
  });

  // ---- project scope ------------------------------------------------------

  it("offers project pills, marks the active one, and reports selection", () => {
    const onSelectProject = vi.fn();
    render(<Dashboard
        cockpit={cockpit}
        connection="connected"
        selectedProject="api"
        onSelectProject={onSelectProject}
      />);
    expect(screen.getByRole("button", { name: "api" })).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("button", { name: "All" })).not.toHaveAttribute("aria-current");
    fireEvent.click(screen.getByRole("button", { name: "web — has a fault" }));
    expect(onSelectProject).toHaveBeenCalledWith("web");
  });

  it("filters by the selected project", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" selectedProject="api" />);
    expect(rowFor("api/c")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /web\/b/ })).toBeNull();
  });

  it("empty state points at the new-task CTA", () => {
    const docsCockpit: BrowserCockpitView = {
      ...cockpit,
      repos: { repos: [...cockpit.repos.repos, { name: "docs" }] },
    };
    render(<Dashboard cockpit={docsCockpit} connection="connected" selectedProject="docs" />);
    expect(screen.getByText("No tasks in docs yet — start one below.")).toBeInTheDocument();

    const emptyCockpit: BrowserCockpitView = { ...cockpit, cards: [] };
    render(<Dashboard cockpit={emptyCockpit} connection="connected" />);
    expect(screen.getByText("All quiet — start a new task below.")).toBeInTheDocument();
  });

  // ---- the surrounding operational picture --------------------------------

  it("closes the page with repositories and system status", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" />);
    const regions = screen
      .getAllByRole("region")
      .map((region) => region.getAttribute("aria-label"));
    expect(regions).toEqual(["Tasks", "Repositories", "System status"]);
  });

  it("scopes the repository section to the repo route", () => {
    render(<Dashboard cockpit={cockpit} connection="connected" selectedProject="api" />);
    const repos = screen.getByRole("region", { name: "Repositories" });
    expect(within(repos).getByRole("button", { name: /^api/ })).toBeInTheDocument();
    expect(within(repos).queryByRole("button", { name: /^web/ })).toBeNull();
  });

  it("shows the whole fleet's size in system status, not the filtered slice", () => {
    render(<Dashboard cockpit={cockpit} connection="reconnecting" selectedProject="api" />);
    expect(screen.getByTestId("system-link")).toHaveTextContent("Reconnecting");
    const system = screen.getByRole("region", { name: "System status" });
    expect(within(system).getByText(String(cockpit.cards.length))).toBeInTheDocument();
  });

  it("keeps both panels when there are no tasks to show", () => {
    render(<Dashboard cockpit={{ ...cockpit, cards: [] }} connection="connected" />);
    expect(screen.getByText("All quiet — start a new task below.")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Repositories" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "System status" })).toBeInTheDocument();
  });

  // ---- styling contracts --------------------------------------------------

  it("uses accent for the active project pill and danger for the fault dot", () => {
    const activePillRule =
      stylesSource.match(/\.project-pill\.is-active\s*\{([^}]*)\}/)?.[1] ?? "";
    const faultDotRule = stylesSource.match(/\.pill-fault-dot\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(activePillRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
    expect(activePillRule).not.toMatch(/var\(--warn/);
    expect(faultDotRule).toMatch(/var\(--danger\)/);
  });

  it("keeps row action labels on one line so a tap target never reflows", () => {
    expect(stylesSource).toMatch(/\.task-row-actions\s+\.action[\s\S]*?white-space:\s*nowrap/);
  });

  it("drops the swipe-reveal machinery entirely", () => {
    const source = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), "Dashboard.tsx"),
      "utf8",
    );
    expect(source).not.toMatch(/[Ss]wipe/);
  });
});
