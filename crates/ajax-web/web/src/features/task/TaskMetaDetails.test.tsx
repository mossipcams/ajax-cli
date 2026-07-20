import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, screen, waitFor, within } from "@testing-library/react";
import TaskMetaDetails from "./TaskMetaDetails";
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

describe("TaskMetaDetails", () => {
  it("renders created and last-activity relative times in task details", () => {
    const now = Math.floor(Date.now() / 1000);
    render(
      <TaskMetaDetails
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
      <TaskMetaDetails
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
      <TaskMetaDetails detail={detail({ annotations: ["needs rebase", "check CI"] })} />,
    );
    expect(screen.getByTestId("task-annotations").textContent).toContain("needs rebase");
    expect(screen.getByTestId("task-annotations").textContent).toContain("check CI");
  });

  it("omits the annotations block when the task has none", () => {
    render(<TaskMetaDetails detail={detail()} />);
    expect(screen.queryByTestId("task-annotations")).not.toBeInTheDocument();
  });

  it("uses sentence-case field labels without uppercase dt styling", () => {
    const dtBlock = stylesSource.match(/\.detail-grid dt\s*\{([\s\S]*?)\}/);
    expect(dtBlock).not.toBeNull();
    expect(dtBlock![1]).not.toMatch(/text-transform:\s*uppercase/);
  });

  it("flattens task details into one detail grid without group labels", () => {
    render(<TaskMetaDetails detail={detail()} />);
    expect(screen.queryAllByText(/^Branch$/)).toHaveLength(1);
    expect(screen.queryByText(/^Agent$/)).toBeNull();
    expect(screen.queryByText(/^Activity$/)).toBeNull();
  });

  it("renders Attempts as a list heading", () => {
    const now = Math.floor(Date.now() / 1000);
    render(
      <TaskMetaDetails
        detail={detail({
          agent_attempts: [
            { started_unix_secs: now - 600, completed_unix_secs: now - 480, outcome: "completed" },
          ],
        })}
      />,
    );
    expect(screen.getByRole("heading", { name: /attempts/i })).toBeInTheDocument();
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
      <TaskMetaDetails detail={detail({ repo: "ajax-cli", qualified_handle: "ajax-cli/demo" })} />,
    );

    await waitFor(() => {
      expect(screen.getByRole("region", { name: "Test in Dev" })).toBeInTheDocument();
    });

    const detailsGroup = screen.getByRole("group");
    expect(
      within(detailsGroup).getByRole("region", { name: "Test in Dev" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("region").map((region) => region.getAttribute("aria-label"))).toEqual([
      "Test in Dev",
    ]);
  });
});
