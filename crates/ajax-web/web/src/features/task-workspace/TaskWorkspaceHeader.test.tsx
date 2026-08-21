import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import TaskWorkspaceHeader from "./TaskWorkspaceHeader";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../.."));

function detail(overrides: Partial<BrowserTaskDetail> = {}): BrowserTaskDetail {
  return {
    qualified_handle: "web/fix-login",
    repo: "web",
    title: "Fix login",
    branch: "ajax/fix-login",
    base_branch: "main",
    worktree_path: "/repo",
    tmux_session: "ajax-web-fix-login",
    lifecycle: "Reviewable",
    agent: "Codex",
    agent_status: "Idle",
    status: "waiting",
    status_explanation: "Ready",
    actions: [],
    live_status_kind: "WaitingForApproval",
    live_status_summary: "waiting",
    annotations: [],
    created_unix_secs: 0,
    last_activity_unix_secs: 0,
    agent_attempts: [],
    ...overrides,
  };
}

describe("TaskWorkspaceHeader", () => {
  it("renders title, status pill, back, and details affordance", () => {
    const onBack = vi.fn();
    const onOpenDetails = vi.fn();
    render(
      <TaskWorkspaceHeader
        detail={detail()}
        onBack={onBack}
        onOpenDetails={onOpenDetails}
      />,
    );

    expect(screen.getByRole("heading", { name: "Fix login" })).toBeInTheDocument();
    expect(screen.getByTestId("task-details")).toBeInTheDocument();
    expect(screen.getByText("Waiting")).toHaveClass("interact-pill");
    fireEvent.click(screen.getByRole("button", { name: "← Back" }));
    fireEvent.click(screen.getByTestId("task-details"));
    expect(onBack).toHaveBeenCalledOnce();
    expect(onOpenDetails).toHaveBeenCalledOnce();
  });

  it("omits the status pill when showStatusPill is false", () => {
    render(
      <TaskWorkspaceHeader detail={detail()} showStatusPill={false} onBack={vi.fn()} />,
    );
    expect(screen.queryByText("Waiting")).not.toBeInTheDocument();
  });

  it("waiting pill uses a shipped mono mark, not a missing-glyph placeholder (#1020)", () => {
    const waitingRule =
      stylesSource.match(
        /\.interact-pill\.tone-waiting::before[\s\S]*?\}/,
      )?.[0] ?? "";
    expect(waitingRule).not.toMatch(/content:\s*"\?"/);
    expect(waitingRule).toMatch(/content:\s*"◦"/);
  });

  it("renders the handle while task detail is loading", () => {
    render(<TaskWorkspaceHeader handle="web/fix-login" onBack={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "web/fix-login" })).toBeInTheDocument();
    expect(screen.queryByTestId("task-details")).not.toBeInTheDocument();
  });
});
