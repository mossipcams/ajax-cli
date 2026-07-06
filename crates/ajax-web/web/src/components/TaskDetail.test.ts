import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskDetail from "./TaskDetail.svelte";
import taskDetailSource from "./TaskDetail.svelte?raw";
import type { BrowserTaskDetail } from "../types";

vi.mock("ghostty-web", () => ({
  Ghostty: {
    load: vi.fn(() => Promise.resolve({ runtime: "ghostty" })),
  },
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    buffer = { active: { viewportY: 0, baseY: 0 } };
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    onScroll = vi.fn(() => ({ dispose: vi.fn() }));
    scrollToBottom = vi.fn();
    scrollLines = vi.fn();
    focus = vi.fn();
    blur = vi.fn();
    paste = vi.fn();
    resize = vi.fn();
    getViewportY = vi.fn(() => 0);
    options = { fontSize: 13 };
  },
  FitAddon: class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
    proposeDimensions = vi.fn(() => ({ cols: 80, rows: 24 }));
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

  it("defines the mobile task route as a fixed visual-viewport shell", () => {
    const mobileBlock = taskDetailSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    const mobileCss = mobileBlock![1];

    expect(mobileCss).toMatch(/:global\(html\.ajax-task-open\),\s*:global\(html\.ajax-task-open body\)\s*\{[^}]*overflow:\s*hidden/);
    expect(mobileCss).toMatch(/:global\(html\.ajax-task-open\),\s*:global\(html\.ajax-task-open body\)\s*\{[^}]*height:\s*var\(--app-height,\s*100dvh\)/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*position:\s*fixed/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*top:\s*var\(--app-top,\s*0px\)/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*height:\s*100dvh/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*height:\s*var\(--app-height,\s*100dvh\)/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*inset:\s*0/);

    expect(taskDetailSource).toMatch(
      /:global\(html\.terminal-expanded\)\s*\.task-detail \.terminal-primary\s*\{[^}]*top:\s*var\(--app-top,\s*0px\)/,
    );
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*overflow:\s*hidden/);
    expect(mobileCss).toMatch(/\.terminal-primary\s*\{[^}]*min-width:\s*0/);
    expect(mobileCss).toMatch(/\.terminal-primary\s*\{[^}]*width:\s*100%/);
    expect(mobileCss).toMatch(/\.terminal-primary\s*\{[^}]*max-width:\s*100%/);
    expect(mobileCss).toMatch(/\.terminal-primary\s*\{[^}]*overflow:\s*hidden/);
    // Full-bleed terminal: the shell keeps only the top inset; the key bar
    // pads the bottom inset and chrome rows carry their own gutters.
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*padding:\s*env\(safe-area-inset-top\)\s*0\s*0/);
    expect(mobileCss).toMatch(/\.detail-header,\s*\.interact-panel\s*\{[^}]*padding-left:[^;]*env\(safe-area-inset-left\)/);
  });
});
