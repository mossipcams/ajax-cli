import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskDetail from "./TaskDetail.svelte";
import taskDetailSource from "./TaskDetail.svelte?raw";
import type { BrowserTaskDetail } from "../types";

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    buffer = { active: { viewportY: 0, baseY: 0 } };
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn();
    onScroll = vi.fn();
    scrollToBottom = vi.fn();
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock("xterm-zerolag-input", () => ({
  ZerolagInputAddon: class MockZerolagInputAddon {
    getFlushed = vi.fn(() => ({ count: 0, text: "" }));
    setFlushed = vi.fn();
    removeChar = vi.fn();
    clear = vi.fn();
    clearFlushed = vi.fn();
    rerender = vi.fn();
    dispose = vi.fn();
  },
}));

beforeEach(() => {
  vi.stubGlobal("WebSocket", class {
    readyState = 1;
    close() {}
    addEventListener() {}
    send() {}
  });
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

  it("renders the task terminal panel for the qualified handle", () => {
    const { getByTestId } = render(TaskDetail, { props: { detail: detail() } });
    expect(getByTestId("task-terminal-panel")).toBeInTheDocument();
  });

  it("exposes mobile terminal-first layout hooks", () => {
    const { container } = render(TaskDetail, { props: { detail: detail() } });

    expect(container.querySelector(".task-detail.is-terminal-first")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-chrome='header']")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-chrome='actions']")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-primary='terminal']")).toBeInTheDocument();
  });

  it("fires onBack from the back control", async () => {
    const onBack = vi.fn();
    const { getByText } = render(TaskDetail, { props: { detail: detail(), onBack } });
    await fireEvent.click(getByText("← Back"));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("hides the task-details disclosure on mobile so the terminal gets the height", () => {
    // The mobile task view is a fixed-height band; the disclosure below the
    // terminal eats rows the operator asked for. Its facts stay on desktop.
    const mobileBlock = taskDetailSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    expect(mobileBlock![1]).toMatch(/\.meta-details\s*\{[^}]*display:\s*none/);
  });
});
