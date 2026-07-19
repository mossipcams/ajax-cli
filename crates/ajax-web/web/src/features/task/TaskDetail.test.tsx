import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, fireEvent, waitFor, screen, within } from "@testing-library/react";
import TaskDetail from "./TaskDetail";
import taskDetailSource from "./TaskDetail?raw";
import routeScrollSource from "@/app/RouteScroll.tsx?raw";
import appSource from "@/app/App.tsx?raw";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const stylesSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"),
  "utf8",
);

const fetchDevDeploy = vi.fn();

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return {
    ...actual,
    fetchDevDeploy: (...args: unknown[]) => fetchDevDeploy(...args),
  };
});

beforeEach(() => {
  fetchDevDeploy.mockReset();
  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
});

afterEach(() => vi.restoreAllMocks());

function detail(overrides: Partial<BrowserTaskDetail> = {}): BrowserTaskDetail {
  return {
    qualified_handle: "web/fix-login",
    repo: "web",
    title: "Fix login",
    branch: "ajax/fix-login",
    base_branch: "main",
    worktree_path: "/repo/web__worktrees/ajax-fix-login",
    tmux_session: "ajax-web-fix-login",
    lifecycle: "Reviewable",
    agent: "Codex",
    agent_status: "Idle",
    status: "waiting",
    status_explanation: "Ready for review",
    actions: [{ action: "review", label: "Review", destructive: false, confirmation_required: false }],
    live_status_kind: "WaitingForApproval",
    live_status_summary: "waiting",
    annotations: [],
    created_unix_secs: 0,
    last_activity_unix_secs: 0,
    agent_attempts: [],
    ...overrides,
  };
}

function taskDetailMobileBlock(): string {
  const start = stylesSource.indexOf("/* DETAIL HEADER");
  const section = start >= 0 ? stylesSource.slice(start) : stylesSource;
  const match = section.match(
    /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
  );
  return match?.[1] ?? "";
}

describe("TaskDetail", () => {
  it("renders the canonical headline status", () => {
    render(<TaskDetail detail={detail()} />);
    expect(screen.getByText("Waiting")).toHaveClass("interact-pill");
    expect(screen.getByText("Ready for review")).toBeInTheDocument();
  });

  it("renders the ordered actions without inferring them", () => {
    render(<TaskDetail detail={detail()} />);
    expect(screen.getByText("Review")).toBeInTheDocument();
  });

  it("removes redundant resume from task detail actions", () => {
    render(
      <TaskDetail
        detail={detail({
          actions: [
            { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
          ],
        })}
      />,
    );

    expect(screen.queryByText("Resume")).not.toBeInTheDocument();
    expect(screen.getByText("Review")).toBeInTheDocument();
  });

  it("exposes mobile layout hooks for header and actions", () => {
    render(<TaskDetail detail={detail()} />);

    expect(screen.getByTestId("mobile-chrome-header")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-chrome-actions")).toBeInTheDocument();
    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
  });

  it("renders the task outlet hook the scroll lock targets", () => {
    expect(appSource).toMatch(
      /<section[^>]*data-outlet="task"[^>]*>[\s\S]*?<TaskDetail/,
    );
    // `.task-detail` is the element the scroll lock targets; the terminal
    // region is a different node and would not prove this contract.
    render(<TaskDetail detail={detail()} />);
    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
  });

  it("fires onBack from the back control", async () => {
    const onBack = vi.fn();
    render(<TaskDetail detail={detail()} onBack={onBack} />);
    fireEvent.click(screen.getByText("← Back"));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("does not own document scroll via ajax-task-open", () => {
    expect(taskDetailSource).not.toMatch(/ajax-task-open/);
    expect(routeScrollSource).toMatch(/data-testid="route-scroll"/);
  });

  it("does not toggle document classes on mount", () => {
    document.documentElement.classList.remove("ajax-task-open");
    const { unmount } = render(<TaskDetail detail={detail()} />);

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);

    unmount();

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);
  });
});

describe("TaskDetail projection surface", () => {
  it("surfaces the runtime observation error as a warning", () => {
    render(
      <TaskDetail detail={detail({ runtime_observation_error: "tmux capture failed" })} />,
    );
    expect(screen.getByTestId("observation-error").textContent).toContain("tmux capture failed");
  });

  it("omits the observation warning when observation succeeded", () => {
    render(<TaskDetail detail={detail()} />);
    expect(screen.queryByTestId("observation-error")).not.toBeInTheDocument();
  });

  it("shows agent activity when it adds information beyond the status line", () => {
    render(
      <TaskDetail detail={detail({ agent_activity: "running cargo nextest" })} />,
    );
    expect(screen.getByTestId("agent-activity").textContent).toContain("running cargo nextest");
  });

  it("hides agent activity when it just repeats the status explanation", () => {
    render(
      <TaskDetail
        detail={detail({ agent_activity: "Ready for review", status_explanation: "Ready for review" })}
      />,
    );
    expect(screen.queryByTestId("agent-activity")).not.toBeInTheDocument();
  });

  it("falls back to the live status summary for the activity line", () => {
    render(
      <TaskDetail detail={detail({ agent_activity: null, live_status_summary: "waiting on approval" })} />,
    );
    expect(screen.getByTestId("agent-activity").textContent).toContain("waiting on approval");
  });

  it("renders created and last-activity relative times in task details", () => {
    const now = Math.floor(Date.now() / 1000);
    render(
      <TaskDetail
        detail={detail({
          created_unix_secs: now - 2 * 86400,
          last_activity_unix_secs: now - 5 * 60,
        })}
      />,
    );
    expect(screen.getByText("2d ago")).toBeInTheDocument();
    expect(screen.getByText("5m ago")).toBeInTheDocument();
  });

  it("lists agent attempts with outcome and duration", () => {
    const now = Math.floor(Date.now() / 1000);
    render(
      <TaskDetail
        detail={detail({
          agent_attempts: [
            { started_unix_secs: now - 600, completed_unix_secs: now - 480, outcome: "completed" },
            { started_unix_secs: now - 300, completed_unix_secs: null, outcome: "running" },
          ],
        })}
      />,
    );
    const attempts = screen.getByTestId("agent-attempts");
    expect(attempts.textContent).toContain("completed");
    expect(attempts.textContent).toContain("2m");
    expect(attempts.textContent).toContain("running");
  });

  it("lists annotations when the task carries notes", () => {
    render(
      <TaskDetail detail={detail({ annotations: ["needs rebase", "check CI"] })} />,
    );
    expect(screen.getByTestId("task-annotations").textContent).toContain("needs rebase");
    expect(screen.getByTestId("task-annotations").textContent).toContain("check CI");
  });

  it("omits the annotations block when the task has none", () => {
    render(<TaskDetail detail={detail()} />);
    expect(screen.queryByTestId("task-annotations")).not.toBeInTheDocument();
  });

  it("clamps status explanation and activity to a single row", () => {
    const summaryBlock = stylesSource.match(/\.interact-summary\s*\{([\s\S]*?)\}/);
    expect(summaryBlock).not.toBeNull();
    const body = summaryBlock![1];
    expect(body).toMatch(/white-space:\s*nowrap/);
    expect(body).toMatch(/overflow:\s*hidden/);
    expect(body).toMatch(/text-overflow:\s*ellipsis/);
    expect(body).not.toMatch(/overflow-wrap:\s*anywhere/);
  });

  it("keeps the details line flush against the terminal on mobile", () => {
    const mobileBlock = taskDetailMobileBlock();

    expect(mobileBlock).toMatch(/\.meta-details\s*\{[^}]*margin-top:\s*0/);
  });

  it("keeps the mobile interact panel to a single row", () => {
    const mobileBlock = taskDetailMobileBlock();

    const interactPanelCss = [...mobileBlock.matchAll(/\.interact-panel\s*\{([^}]*)\}/g)]
      .map((match) => match[1])
      .join("\n");

    expect(interactPanelCss).toMatch(/flex-direction:\s*row/);
    expect(mobileBlock).toMatch(/\.interact-summary[\s\S]*?min-width:\s*0/);
    expect(mobileBlock).toMatch(/\.interact-summary[\s\S]*?text-overflow:\s*ellipsis/);
  });

  it("shows Test in Dev inside Task details (not on the always-visible page) for ajax-cli tasks", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        active: false,
        error: null,
        occupant: null,
      },
    });

    render(
      <TaskDetail detail={detail({ repo: "ajax-cli", qualified_handle: "ajax-cli/demo" })} />,
    );

    await waitFor(() => {
      expect(screen.getByRole("region", { name: "Task terminal" })).toBeInTheDocument();
      expect(screen.getByRole("region", { name: "Test in Dev" })).toBeInTheDocument();
    });

    const detailsGroup = screen.getByRole("group");
    expect(
      within(detailsGroup).getByRole("region", { name: "Test in Dev" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("region").map((region) => region.getAttribute("aria-label"))).toEqual([
      "Task terminal",
      "Test in Dev",
    ]);
  });

  it("compacts the mobile status panel and action buttons", () => {
    const mobileBlock = taskDetailMobileBlock();

    const interactPanelCss = [...mobileBlock.matchAll(/\.interact-panel\s*\{([^}]*)\}/g)]
      .map((match) => match[1])
      .join("\n");

    expect(interactPanelCss).toMatch(/padding(?:-top)?:\s*[0-4]px/);
    expect(interactPanelCss).toMatch(/margin-top:\s*[0-4]px/);
    expect(interactPanelCss).toMatch(/min-height:\s*0/);
    expect(mobileBlock).toMatch(
      /\.interact-panel\s+\.action[\s\S]*?min-height:\s*(?:2[0-9]|3[0-2])px/,
    );
    expect(mobileBlock).toMatch(/\.interact-panel\s+\.action[\s\S]*?padding:\s*[0-4]px/);
  });
});
