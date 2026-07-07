import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskDetail from "./TaskDetail.svelte";
import taskDetailSource from "./TaskDetail.svelte?raw";
import terminalRawViewSource from "./TerminalRawView.svelte?raw";
import routeScrollSource from "./RouteScroll.svelte?raw";
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
    const mobileBlock = taskDetailSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    expect(mobileBlock![1]).toMatch(/\.meta-details\s*\{[^}]*display:\s*none/);
  });

  it("does not own document scroll via ajax-task-open", () => {
    expect(taskDetailSource).not.toMatch(/ajax-task-open/);
    expect(routeScrollSource).toMatch(/data-testid="route-scroll"/);
  });

  it("defines mobile overlay height pins without a fixed task shell", () => {
    const mobileBlock = taskDetailSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    const mobileCss = mobileBlock![1];

    expect(mobileCss).not.toMatch(/ajax-task-open/);
    expect(mobileCss).toMatch(/:global\(html\.terminal-expanded\),\s*:global\(html\.terminal-expanded body\),\s*:global\(html\.keyboard-open\),\s*:global\(html\.keyboard-open body\)\s*\{[^}]*overflow:\s*hidden/);
    expect(mobileCss).not.toMatch(
      /:global\(html\.terminal-expanded\),\s*:global\(html\.terminal-expanded body\),\s*:global\(html\.keyboard-open\),\s*:global\(html\.keyboard-open body\)\s*\{[^}]*height:\s*var\(--app-height/,
    );
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*min-height:\s*var\(--app-band-height,\s*100dvh\)/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*position:\s*fixed/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*inset:\s*0/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*overflow:\s*hidden/);

    expect(terminalRawViewSource).toMatch(/terminal-inline-spacer/);
    expect(terminalRawViewSource).toMatch(/class:is-expanded=\{expanded\}/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*padding:\s*env\(safe-area-inset-top\)\s*0\s*0/);
    expect(mobileCss).toMatch(/\.detail-header,\s*\.interact-panel\s*\{[^}]*padding-left:[^;]*env\(safe-area-inset-left\)/);
  });

  it("does not toggle document classes on mount", () => {
    document.documentElement.classList.remove("ajax-task-open");
    const { unmount } = render(TaskDetail, { props: { detail: detail() } });

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);

    unmount();

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);
  });
});
