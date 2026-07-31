import { describe, it, expect, vi, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fireEvent, render, screen, within } from "@testing-library/react";
import Dashboard from "./Dashboard";
import type {
  AttentionBand,
  BrowserCockpitView,
  BrowserTaskCard,
  TaskStatus,
} from "@/shared/lib/types";

const here = dirname(fileURLToPath(import.meta.url));
const COCKPIT_CONTRACT = JSON.parse(
  readFileSync(join(here, "../../fixtures/cockpit.json"), "utf8"),
) as BrowserCockpitView;

afterEach(() => vi.restoreAllMocks());

const nowSecs = () => Math.floor(Date.now() / 1000);

function card(overrides: Partial<BrowserTaskCard> = {}): BrowserTaskCard {
  const handle = overrides.qualified_handle ?? "web/fix-login";
  return {
    id: handle,
    qualified_handle: handle,
    repo: handle.split("/")[0],
    title: "Fix login",
    status: "waiting" as TaskStatus,
    status_explanation: "Waiting for approval",
    attention: "needs-you" as AttentionBand,
    last_activity_unix_secs: nowSecs() - 30,
    actions: [
      { action: "review", label: "Review", destructive: false, confirmation_required: false },
    ],
    ...overrides,
  };
}

function cockpit(overrides: Partial<BrowserCockpitView> = {}): BrowserCockpitView {
  return {
    backend: { authority: "host-native", control_enabled: true, warning: null },
    repos: { repos: [{ name: "web", path: "/repo/web" }] },
    cards: [card()],
    inbox: { items: [] },
    ...overrides,
  };
}

function renderDashboard(view: BrowserCockpitView, props: Record<string, unknown> = {}) {
  return render(<Dashboard cockpit={view} connection="connected" {...props} />);
}

const rowHandles = () =>
  screen.queryAllByTestId(/^task-row-/).map((row) => row.getAttribute("data-handle"));

const rail = () => screen.getByTestId("task-rail");

describe("Dashboard — the roster", () => {
  it("renders one line per task, ordered needs-you, running, ready, recent", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "a/idle", attention: "idle", status: "idle" }),
          card({ qualified_handle: "b/review", attention: "review" }),
          card({ qualified_handle: "c/active", attention: "active", status: "running" }),
          card({ qualified_handle: "d/needs", attention: "needs-you" }),
        ],
      }),
    );

    expect(rowHandles()).toEqual(["d/needs", "c/active", "b/review", "a/idle"]);
  });

  it("divides bands with a labelled rule carrying its count", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "d/needs" }),
          card({ qualified_handle: "e/needs-too" }),
          card({ qualified_handle: "c/active", attention: "active", status: "running" }),
        ],
      }),
    );

    const rules = screen.getAllByTestId(/^band-rule-/);
    expect(rules.map((rule) => rule.textContent)).toEqual(["Needs you2", "Running1"]);
    expect(rules.map((rule) => rule.dataset.testid)).toEqual([
      "band-rule-needs-you",
      "band-rule-active",
    ]);
  });

  it("carries the fleet's shape as words in the head, never a gauge", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "d/needs" }),
          card({ qualified_handle: "c/active", attention: "active", status: "running" }),
        ],
      }),
    );

    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("tasks")).toBeInTheDocument();
    expect(screen.getByText("1 needs you · 1 running")).toBeInTheDocument();
  });

  it("keeps the row itself terse — the title belongs to the rail", () => {
    renderDashboard(cockpit());

    const row = screen.getByTestId("task-row-web/fix-login");
    expect(row).toHaveTextContent("web/fix-login");
    expect(within(row).queryByText("Fix login")).not.toBeInTheDocument();
    // A screen reader still gets band and state from the row itself.
    expect(row).toHaveAttribute(
      "aria-label",
      "web/fix-login. Needs you. Waiting for approval",
    );
  });

  it("selects rather than navigates when a row is tapped", () => {
    const onOpenTask = vi.fn();
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "d/needs" }),
          card({ qualified_handle: "c/active", attention: "active", status: "running" }),
        ],
      }),
      { onOpenTask },
    );

    fireEvent.click(screen.getByTestId("task-row-c/active"));

    expect(onOpenTask).not.toHaveBeenCalled();
    expect(rail()).toHaveAttribute("data-handle", "c/active");
    expect(screen.getByTestId("task-row-c/active")).toHaveAttribute("aria-current", "true");
    expect(screen.getByTestId("task-row-d/needs")).not.toHaveAttribute("aria-current");
  });

  it("stops the running glyph pulsing once a task has gone silent", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({
            status: "running",
            status_explanation: "Agent working",
            attention: "active",
            last_activity_unix_secs: nowSecs() - 600,
          }),
        ],
      }),
    );

    expect(screen.getByTestId("task-row-web/fix-login")).toHaveClass("is-quiet");
    expect(within(rail()).getByText(/Stale 10m — no output/)).toBeInTheDocument();
    expect(within(rail()).queryByText("Agent working")).not.toBeInTheDocument();
  });
});

describe("Dashboard — the rail", () => {
  it("opens on the host's leading inbox entry, not the first row", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "d/needs", title: "Needs me" }),
          card({ qualified_handle: "web/fix-login", title: "Fix login" }),
        ],
        // Rust sorts inbox by severity in projection.rs; the browser reads that
        // order and never ranks severity itself.
        inbox: {
          items: [
            { task_handle: "web/fix-login", severity: 1 },
            { task_handle: "d/needs", severity: 4 },
          ],
        },
      }),
    );

    expect(rail()).toHaveAttribute("data-handle", "web/fix-login");
    expect(within(rail()).getByText("Fix login")).toBeInTheDocument();
    expect(within(rail()).getByText("Waiting for approval")).toBeInTheDocument();
  });

  it("falls back to the first row when the host projects no inbox", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "c/active", attention: "active", status: "running" }),
          card({ qualified_handle: "d/needs" }),
        ],
      }),
    );

    expect(rail()).toHaveAttribute("data-handle", "d/needs");
  });

  it("returns to the host's answer when the pinned task leaves the view", () => {
    const both = cockpit({
      cards: [card({ qualified_handle: "d/needs" }), card({ qualified_handle: "web/fix-login" })],
    });
    const { rerender } = renderDashboard(both);

    fireEvent.click(screen.getByTestId("task-row-web/fix-login"));
    expect(rail()).toHaveAttribute("data-handle", "web/fix-login");

    // The next poll drops that task (a Drop landed elsewhere).
    rerender(
      <Dashboard
        cockpit={{ ...both, cards: [card({ qualified_handle: "d/needs" })] }}
        connection="connected"
      />,
    );

    expect(rail()).toHaveAttribute("data-handle", "d/needs");
  });

  it("carries every safe action, and never Drop or Resume", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({
            actions: [
              {
                action: "resume",
                label: "Resume",
                destructive: false,
                confirmation_required: false,
              },
              { action: "ship", label: "Ship", destructive: false, confirmation_required: false },
              {
                action: "review",
                label: "Review",
                destructive: false,
                confirmation_required: false,
              },
              { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
            ],
          }),
        ],
      }),
    );

    expect(within(rail()).getByRole("button", { name: "Ship" })).toBeInTheDocument();
    expect(within(rail()).getByRole("button", { name: "Review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Drop" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
  });

  it("says so when the host offers no safe action from here", () => {
    renderDashboard(cockpit({ cards: [card({ actions: [] })] }));

    expect(within(rail()).getByText(/No safe action from here/)).toBeInTheDocument();
    expect(screen.queryByTestId("rail-actions")).not.toBeInTheDocument();
  });

  it("opens the task from the rail, deliberately", () => {
    const onOpenTask = vi.fn();
    renderDashboard(cockpit(), { onOpenTask });

    fireEvent.click(within(rail()).getByRole("button", { name: /Open/ }));

    expect(onOpenTask).toHaveBeenCalledWith("web/fix-login");
  });

  it("reserves exactly the rail's height at the roster's tail", () => {
    renderDashboard(cockpit());

    // jsdom reports offsetHeight 0, so the component keeps its fallback rather
    // than collapsing the clearance and letting the rail cover the last row.
    expect(screen.getByTestId("rail-clearance").style.height).toBe("148px");
  });
});

describe("Dashboard — scoping", () => {
  it("routes repo selection through the native picker", () => {
    const onSelectProject = vi.fn();
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "web/fix-login" }),
          card({ qualified_handle: "api/add-auth" }),
        ],
        repos: { repos: [{ name: "web" }, { name: "api" }] },
      }),
      { onSelectProject },
    );

    fireEvent.change(screen.getByTestId("repo-select"), { target: { value: "api" } });
    expect(onSelectProject).toHaveBeenCalledWith("api");

    fireEvent.change(screen.getByTestId("repo-select"), { target: { value: "" } });
    expect(onSelectProject).toHaveBeenLastCalledWith(null);
  });

  it("shows only the selected repo's tasks", () => {
    renderDashboard(
      cockpit({
        cards: [
          card({ qualified_handle: "web/fix-login" }),
          card({ qualified_handle: "api/add-auth" }),
        ],
      }),
      { selectedProject: "api" },
    );

    expect(rowHandles()).toEqual(["api/add-auth"]);
    expect(rail()).toHaveAttribute("data-handle", "api/add-auth");
  });

  it("invites a first task when the fleet is empty, with no rail to act on", () => {
    renderDashboard(cockpit({ cards: [] }));

    expect(screen.getByText(/All quiet — start a task with New/)).toBeInTheDocument();
    expect(screen.queryByTestId("task-rail")).not.toBeInTheDocument();
    expect(screen.queryByTestId("roster")).not.toBeInTheDocument();
  });

  it("scopes the empty state to the selected repo", () => {
    renderDashboard(cockpit({ cards: [] }), { selectedProject: "web" });

    expect(screen.getByText(/No tasks in web/)).toBeInTheDocument();
  });
});

describe("Dashboard — system footer", () => {
  it("stays closed and reports authority and control, with link state as a dot", () => {
    renderDashboard(cockpit());

    const footer = screen.getByTestId("fleet-footer");
    expect(footer).not.toHaveAttribute("open");
    expect(within(footer).getAllByText("host-native").length).toBeGreaterThan(0);
    expect(within(footer).getByText("enabled")).toBeInTheDocument();
    // Link state belongs to the header's ConnectionStatus; the footer only tints
    // its dot, so the word is never printed twice on one screen.
    expect(screen.getByTestId("fleet-link-dot")).toHaveAttribute("data-live", "true");
    expect(within(footer).queryByText("connected")).toBeNull();
  });

  it("leaves the dot untinted when the link is not connected", () => {
    renderDashboard(cockpit(), { connection: "reconnecting" });

    expect(screen.getByTestId("fleet-link-dot")).toHaveAttribute("data-live", "false");
  });

  it("surfaces the backend warning the server projected", () => {
    renderDashboard(
      cockpit({
        backend: {
          authority: "host-native",
          control_enabled: false,
          warning: "runtime probe failed",
        },
      }),
    );

    const footer = screen.getByTestId("fleet-footer");
    expect(within(footer).getByText("runtime probe failed")).toBeInTheDocument();
    expect(within(footer).getByText("read-only")).toBeInTheDocument();
  });

  it("renders each repo's server-projected counts, and 'quiet' when all are zero", () => {
    renderDashboard(
      cockpit({
        repos: {
          repos: [
            {
              name: "web",
              path: "/repo/web",
              active_tasks: 2,
              attention_items: 1,
              reviewable_tasks: 0,
              cleanable_tasks: 3,
            },
            { name: "api", path: "/repo/api", active_tasks: 0, attention_items: 0 },
          ],
        },
      }),
    );

    const web = within(screen.getByTestId("repo-line-web"));
    expect(web.getByText("2 active")).toBeInTheDocument();
    expect(web.getByText("1 needs you")).toBeInTheDocument();
    expect(web.getByText("3 cleanable")).toBeInTheDocument();
    expect(web.queryByText(/ready/)).toBeNull();
    expect(within(screen.getByTestId("repo-line-api")).getByText("quiet")).toBeInTheDocument();
  });

  it("collapses to the selected repo on a repo route", () => {
    renderDashboard(cockpit({ repos: { repos: [{ name: "web" }, { name: "api" }] } }), {
      selectedProject: "web",
    });

    expect(screen.getByTestId("repo-line-web")).toBeInTheDocument();
    expect(screen.queryByTestId("repo-line-api")).not.toBeInTheDocument();
  });

  it("hands diagnostics to the settings route", () => {
    const onOpenSettings = vi.fn();
    renderDashboard(cockpit(), { onOpenSettings });

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));

    expect(onOpenSettings).toHaveBeenCalled();
  });
});

describe("Dashboard — Rust contract fixture", () => {
  // The committed fixture is asserted byte-for-byte against production
  // serialization in slices/cockpit.rs, so this renders what the server sends.
  it("renders the committed cockpit projection", () => {
    renderDashboard(COCKPIT_CONTRACT);

    expect(rowHandles()).toEqual(["web/fix-login"]);
    expect(rail()).toHaveAttribute("data-handle", "web/fix-login");
    expect(within(rail()).getByText("Waiting for approval")).toBeInTheDocument();
    expect(within(rail()).getByRole("button", { name: "Ship" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Drop" })).not.toBeInTheDocument();
  });
});
