import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskDetail from "./TaskDetail.svelte";
import taskDetailSource from "./TaskDetail.svelte?raw";
import routeScrollSource from "./RouteScroll.svelte?raw";
import appSource from "./App.svelte?raw";
import type { BrowserTaskDetail } from "../types";

beforeEach(() => {
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

describe("TaskDetail", () => {
  it("renders the canonical headline status", () => {
    const { getByText, container } = render(TaskDetail, { props: { detail: detail() } });
    expect(container.querySelector(".interact-pill")?.textContent).toContain("Waiting");
    expect(getByText("Ready for review")).toBeInTheDocument();
  });

  it("renders the ordered actions without inferring them", () => {
    const { getByText } = render(TaskDetail, { props: { detail: detail() } });
    expect(getByText("Review")).toBeInTheDocument();
  });

  it("removes redundant resume from task detail actions", () => {
    const { getByText, queryByText } = render(TaskDetail, {
      props: {
        detail: detail({
          actions: [
            { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
          ],
        }),
      },
    });

    expect(queryByText("Resume")).not.toBeInTheDocument();
    expect(getByText("Review")).toBeInTheDocument();
  });

  it("exposes mobile layout hooks for header and actions", () => {
    const { container } = render(TaskDetail, { props: { detail: detail() } });

    expect(container.querySelector("[data-mobile-chrome='header']")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-chrome='actions']")).toBeInTheDocument();
    expect(container.querySelector(".task-detail")).toBeInTheDocument();
  });

  it("renders the task outlet hook the scroll lock targets", () => {
    expect(appSource).toMatch(
      /<section[^>]*data-outlet="task"[^>]*>[\s\S]*?<TaskDetail\b/,
    );
    const { container } = render(TaskDetail, { props: { detail: detail() } });
    expect(container.querySelector(".task-detail")).toBeInTheDocument();
  });

  it("fires onBack from the back control", async () => {
    const onBack = vi.fn();
    const { getByText } = render(TaskDetail, { props: { detail: detail(), onBack } });
    await fireEvent.click(getByText("← Back"));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("does not own document scroll via ajax-task-open", () => {
    expect(taskDetailSource).not.toMatch(/ajax-task-open/);
    expect(routeScrollSource).toMatch(/data-testid="route-scroll"/);
  });

  it("does not toggle document classes on mount", () => {
    document.documentElement.classList.remove("ajax-task-open");
    const { unmount } = render(TaskDetail, { props: { detail: detail() } });

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);

    unmount();

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);
  });
});

describe("TaskDetail projection surface", () => {
  it("surfaces the runtime observation error as a warning", () => {
    const { getByTestId } = render(TaskDetail, {
      props: { detail: detail({ runtime_observation_error: "tmux capture failed" }) },
    });
    expect(getByTestId("observation-error").textContent).toContain("tmux capture failed");
  });

  it("omits the observation warning when observation succeeded", () => {
    const { queryByTestId } = render(TaskDetail, { props: { detail: detail() } });
    expect(queryByTestId("observation-error")).not.toBeInTheDocument();
  });

  it("shows agent activity when it adds information beyond the status line", () => {
    const { getByTestId } = render(TaskDetail, {
      props: { detail: detail({ agent_activity: "running cargo nextest" }) },
    });
    expect(getByTestId("agent-activity").textContent).toContain("running cargo nextest");
  });

  it("hides agent activity when it just repeats the status explanation", () => {
    const { queryByTestId } = render(TaskDetail, {
      props: {
        detail: detail({ agent_activity: "Ready for review", status_explanation: "Ready for review" }),
      },
    });
    expect(queryByTestId("agent-activity")).not.toBeInTheDocument();
  });

  it("falls back to the live status summary for the activity line", () => {
    const { getByTestId } = render(TaskDetail, {
      props: { detail: detail({ agent_activity: null, live_status_summary: "waiting on approval" }) },
    });
    expect(getByTestId("agent-activity").textContent).toContain("waiting on approval");
  });

  it("renders created and last-activity relative times in task details", () => {
    const now = Math.floor(Date.now() / 1000);
    const { getByText } = render(TaskDetail, {
      props: {
        detail: detail({
          created_unix_secs: now - 2 * 86400,
          last_activity_unix_secs: now - 5 * 60,
        }),
      },
    });
    expect(getByText("2d ago")).toBeInTheDocument();
    expect(getByText("5m ago")).toBeInTheDocument();
  });

  it("lists agent attempts with outcome and duration", () => {
    const now = Math.floor(Date.now() / 1000);
    const { getByTestId } = render(TaskDetail, {
      props: {
        detail: detail({
          agent_attempts: [
            { started_unix_secs: now - 600, completed_unix_secs: now - 480, outcome: "completed" },
            { started_unix_secs: now - 300, completed_unix_secs: null, outcome: "running" },
          ],
        }),
      },
    });
    const attempts = getByTestId("agent-attempts");
    expect(attempts.textContent).toContain("completed");
    expect(attempts.textContent).toContain("2m");
    expect(attempts.textContent).toContain("running");
  });

  it("lists annotations when the task carries notes", () => {
    const { getByTestId } = render(TaskDetail, {
      props: { detail: detail({ annotations: ["needs rebase", "check CI"] }) },
    });
    expect(getByTestId("task-annotations").textContent).toContain("needs rebase");
    expect(getByTestId("task-annotations").textContent).toContain("check CI");
  });

  it("omits the annotations block when the task has none", () => {
    const { queryByTestId } = render(TaskDetail, { props: { detail: detail() } });
    expect(queryByTestId("task-annotations")).not.toBeInTheDocument();
  });

  it("clamps status explanation and activity to a single row", () => {
    const summaryBlock = taskDetailSource.match(/\.interact-summary\s*\{([\s\S]*?)\}/);
    expect(summaryBlock).not.toBeNull();
    const body = summaryBlock![1];
    expect(body).toMatch(/white-space:\s*nowrap/);
    expect(body).toMatch(/overflow:\s*hidden/);
    expect(body).toMatch(/text-overflow:\s*ellipsis/);
    expect(body).not.toMatch(/overflow-wrap:\s*anywhere/);
  });
});
