import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskDetail from "./TaskDetail.svelte";
import * as api from "../api";
import type { BrowserTaskDetail } from "../types";

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
    vi.spyOn(api, "fetchPane").mockResolvedValue({ kind: "missing" });
    const { getByText, container } = render(TaskDetail, { props: { detail: detail() } });
    expect(container.querySelector(".interact-pill")?.textContent).toContain("Waiting");
    expect(getByText("Ready for review")).toBeInTheDocument();
  });

  it("renders the ordered actions without inferring them", () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue({ kind: "missing" });
    const { getByText } = render(TaskDetail, { props: { detail: detail() } });
    expect(getByText("Review")).toBeInTheDocument();
  });

  it("fires onBack from the back control", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue({ kind: "missing" });
    const onBack = vi.fn();
    const { getByText } = render(TaskDetail, { props: { detail: detail(), onBack } });
    await fireEvent.click(getByText("← Back"));
    expect(onBack).toHaveBeenCalledOnce();
  });
});
