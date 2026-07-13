import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import TaskDetail from "./TaskDetail.svelte";
import taskDetailSource from "./TaskDetail.svelte?raw";
import terminalRawViewSource from "./TerminalRawView.svelte?raw";
import routeScrollSource from "./RouteScroll.svelte?raw";
import appSource from "./App.svelte?raw";
import type { BrowserTaskDetail } from "../types";

// Vite returns "" for `*.css?raw` under vitest, so read the stylesheet from disk.
function loadStylesSource(): string {
  const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
  return readFileSync(join(testDir, "../styles.css"), "utf8");
}

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

  it("renders the task terminal panel for the qualified handle", async () => {
    const { findByTestId } = render(TaskDetail, { props: { detail: detail() } });
    expect(await findByTestId("task-terminal-panel")).toBeInTheDocument();
  });

  it("exposes mobile terminal-first layout hooks", () => {
    const { container } = render(TaskDetail, { props: { detail: detail() } });

    expect(container.querySelector(".task-detail.is-terminal-first")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-chrome='header']")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-chrome='actions']")).toBeInTheDocument();
    expect(container.querySelector("[data-mobile-primary='terminal']")).toBeInTheDocument();
  });

  it("renders the task outlet hook the scroll lock targets", () => {
    // Characterization: App owns `[data-outlet="task"]`; TaskDetail alone cannot
    // mount that wrapper. Pin the markup the mobile `:has()` scroll lock keys off.
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
    // Document-level keyboard/expanded scroll policy lives in styles.css, not TaskDetail.
    expect(taskDetailSource).not.toMatch(/:global\(html\.keyboard-open/);
    expect(taskDetailSource).not.toMatch(/:global\(html\.terminal-expanded/);
    // Fill the locked route-scroll band; do not force app-band min-height (that
    // plus route-scroll padding made the page scroll outside the terminal).
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*min-height:\s*0/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*flex:\s*1\s+1\s+auto/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*overflow:\s*hidden/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*position:\s*fixed/);
    expect(mobileCss).not.toMatch(/\.task-detail\s*\{[^}]*inset:\s*0/);

    expect(terminalRawViewSource).toMatch(/terminal-inline-spacer/);
    expect(terminalRawViewSource).toMatch(/class:is-expanded=\{expanded\}/);
    expect(mobileCss).toMatch(/\.task-detail\s*\{[^}]*padding:\s*env\(safe-area-inset-top\)\s*0\s*0/);
    expect(mobileCss).toMatch(/\.detail-header,\s*\.interact-panel\s*\{[^}]*padding-left:[^;]*env\(safe-area-inset-left\)/);

    const stylesSource = loadStylesSource();
    expect(stylesSource).toMatch(/html\.keyboard-open \.cockpit-chrome/);
    expect(stylesSource).toMatch(/html\.keyboard-open \.bottom-nav/);
    expect(stylesSource).toMatch(/html\.terminal-expanded \.cockpit-chrome/);
    expect(stylesSource).toMatch(/html\.terminal-expanded \.bottom-nav/);
    expect(stylesSource).toMatch(
      /html\.keyboard-open \[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \[data-testid="task-terminal-panel"\]\.is-expanded/,
    );

    const mobileStylesBlocks = [...stylesSource.matchAll(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n\}/g,
    )];
    const mobileExpandedPanelRule = mobileStylesBlocks
      .map((match) => match[1])
      .find((block) =>
        block.includes('html.terminal-expanded [data-testid="task-terminal-panel"].is-expanded'),
      );
    expect(mobileExpandedPanelRule).toBeDefined();
    const expandedPanelBlock = mobileExpandedPanelRule!.match(
      /html\.terminal-expanded \[data-testid="task-terminal-panel"\]\.is-expanded\s*\{([^}]*)\}/,
    )?.[1];
    expect(expandedPanelBlock).toBeDefined();
    expect(expandedPanelBlock!).toMatch(/top:\s*0(px)?;/);
    expect(expandedPanelBlock!).not.toMatch(/top:\s*var\(--app-band-top/);
    expect(expandedPanelBlock!).toMatch(/height:\s*var\(--app-band-height/);
  });

  it("mobile task terminal panel clears the 58vh max-height so it can flex-fill", () => {
    const stylesSource = loadStylesSource();
    const mobileBlocks = [...stylesSource.matchAll(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n\}/g,
    )];
    const mobileCss = mobileBlocks
      .map((match) => match[1])
      .find((block) => block.includes(".task-detail .terminal-panel"));
    expect(mobileCss).toBeDefined();

    expect(mobileCss!).toMatch(
      /\.task-detail \.terminal-panel,\s*\.task-detail \[data-testid="task-terminal-panel"\]\s*\{[^}]*max-height:\s*none/,
    );
    expect(mobileCss!).not.toMatch(
      /\.task-detail \.terminal-panel,\s*\.task-detail \[data-testid="task-terminal-panel"\]\s*\{[^}]*max-height:\s*min\(58vh,\s*560px\)/,
    );

    expect(stylesSource).toMatch(
      /@media \(min-width: 768px\) and \(not \(\(pointer: coarse\) and \(max-height: 500px\)\)\) \{[\s\S]*\.task-detail \.terminal-panel,\s*\.task-detail \[data-testid="task-terminal-panel"\]\s*\{[^}]*max-height:\s*min\(58vh,\s*560px\)/,
    );
    expect(terminalRawViewSource).toMatch(
      /@media \(min-width: 768px\)[\s\S]*height:\s*min\(58vh,\s*560px\)/,
    );
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
});
