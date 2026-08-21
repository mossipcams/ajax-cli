import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { useState, type ComponentProps } from "react";
import { render, fireEvent, screen, act, within } from "@testing-library/react";
import TaskTerminalView from "./TaskTerminalView";
import TaskDetailsSheet from "@/features/task-workspace/TaskDetailsSheet";
import taskTerminalViewSource from "./TaskTerminalView?raw";
import taskTerminalSource from "@/features/terminal/TaskTerminal?raw";
import routeScrollSource from "@/app/RouteScroll.tsx?raw";
import appSource from "@/app/App.tsx?raw";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import { setSwipeEnterDirection } from "@/shared/lib/swipeEnter";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";

vi.mock("@/shared/lib/swipeEnter", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/swipeEnter")>(
    "@/shared/lib/swipeEnter",
  );
  return {
    ...actual,
    setSwipeEnterDirection: vi.fn(actual.setSwipeEnterDirection),
  };
});

const stylesSource = readOrderedStylesSource(
  join(dirname(fileURLToPath(import.meta.url)), "../.."),
);

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
  vi.mocked(setSwipeEnterDirection).mockClear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

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

function TaskTerminalViewWithSheet(
  props: ComponentProps<typeof TaskTerminalView> & {
    orchestrationChat?: boolean;
    onOpenChat?: () => void;
  },
) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const panelId = "test-task-panel";
  const { orchestrationChat = false, onOpenChat, ...taskDetailProps } = props;
  return (
    <>
      <TaskTerminalView
        {...taskDetailProps}
        onOpenDetails={() => setDetailsOpen(true)}
        detailsOpen={detailsOpen}
        detailsPanelId={panelId}
      />
      <TaskDetailsSheet
        open={detailsOpen}
        onOpenChange={setDetailsOpen}
        panelId={panelId}
        mode="terminal"
        detail={taskDetailProps.detail}
        orchestrationChat={orchestrationChat}
        onOpenChat={onOpenChat}
        onResult={taskDetailProps.onResult}
      />
    </>
  );
}

function renderWithSheet(props: ComponentProps<typeof TaskTerminalView>) {
  return render(<TaskTerminalViewWithSheet {...props} />);
}

describe("TaskTerminalView", () => {
  it("renders the canonical headline status", () => {
    render(<TaskTerminalView detail={detail()} />);
    expect(screen.getByText("Waiting")).toHaveClass("interact-pill");
    expect(screen.getByText("Ready for review")).toBeInTheDocument();
  });

  it("renders the ordered actions without inferring them", () => {
    render(<TaskTerminalView detail={detail()} />);
    expect(screen.getByText("Review")).toBeInTheDocument();
  });

  it("removes redundant resume from task detail actions", () => {
    render(
      <TaskTerminalView
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
    render(<TaskTerminalView detail={detail()} />);

    expect(screen.getByTestId("mobile-chrome-header")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-chrome-actions")).toBeInTheDocument();
    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
  });

  it("exposes a header Details control matching the session chat live head", () => {
    render(<TaskTerminalView detail={detail()} onOpenDetails={vi.fn()} />);
    expect(screen.getByTestId("task-details")).toHaveClass("session-head-details");
    expect(screen.getByTestId("task-details")).toHaveTextContent("Details");
  });

  it("does not show the harness switch on the terminal task page", () => {
    render(<TaskTerminalView detail={detail({ agent: "cursor" })} />);
    expect(screen.queryByTestId("harness-swap")).not.toBeInTheDocument();
  });

  it("shows Ajax chat in the header Details sheet when orchestration chat is enabled and the task is session-capable", () => {
    const onOpenChat = vi.fn();
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: true,
      onOpenChat,
    });
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("task-details"));
    const sheet = screen.getByTestId("task-details-sheet");
    const primaryTools = within(sheet).getByTestId("task-primary-tools");
    expect(primaryTools).toHaveClass("session-sheet-tools-primary");
    fireEvent.click(within(primaryTools).getByRole("button", { name: "Ajax chat" }));
    expect(onOpenChat).toHaveBeenCalledOnce();
  });

  it("pins Ajax chat primary tools outside the scrolling session details body", () => {
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: true,
      onOpenChat: vi.fn(),
    });
    fireEvent.click(screen.getByTestId("task-details"));
    const sheet = screen.getByTestId("task-details-sheet");
    const primaryTools = within(sheet).getByTestId("task-primary-tools");
    const body = within(sheet).getByTestId("session-details-body");
    const meta = within(sheet).getByTestId("task-meta-details-embedded");
    expect(body).not.toContainElement(primaryTools);
    expect(primaryTools).toHaveClass("session-sheet-tools-primary");
    expect(primaryTools.compareDocumentPosition(meta) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(body).toContainElement(meta);
    expect(within(sheet).queryByTestId("task-ajax-chat-action")).not.toBeInTheDocument();
  });

  it("keeps the header Details control reachable while terminal-expanded", () => {
    document.documentElement.classList.add("terminal-expanded");
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: true,
      onOpenChat: vi.fn(),
    });
    const details = screen.getByTestId("task-details");
    expect(details).toBeInTheDocument();
    fireEvent.click(details);
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
    document.documentElement.classList.remove("terminal-expanded");
  });

  it("reaches Ajax chat via header Details without opening inline metadata", () => {
    const onOpenChat = vi.fn();
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: true,
      onOpenChat,
    });
    expect(screen.queryByTestId("task-meta-details-embedded")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("task-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
    fireEvent.click(
      within(screen.getByTestId("task-details-sheet")).getByRole("button", { name: "Ajax chat" }),
    );
    expect(onOpenChat).toHaveBeenCalledOnce();
  });

  it("shows Ajax chat when opening task details from the footer affordance", () => {
    const onOpenChat = vi.fn();
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: true,
      onOpenChat,
    });
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("task-meta-details-trigger"));
    const sheet = screen.getByTestId("task-details-sheet");
    fireEvent.click(within(sheet).getByRole("button", { name: "Ajax chat" }));
    expect(onOpenChat).toHaveBeenCalledOnce();
  });

  it("hides Ajax chat when orchestration chat is off or the task is not session-capable", () => {
    renderWithSheet({
      detail: detail({ session_capable: true }),
      orchestrationChat: false,
      onOpenChat: vi.fn(),
    });
    fireEvent.click(screen.getByTestId("task-details"));
    expect(screen.queryByRole("button", { name: "Ajax chat" })).not.toBeInTheDocument();

    renderWithSheet({
      detail: detail({ session_capable: false }),
      orchestrationChat: true,
      onOpenChat: vi.fn(),
    });
    fireEvent.click(screen.getAllByTestId("task-details").at(-1)!);
    expect(screen.queryAllByRole("button", { name: "Ajax chat" })).toHaveLength(0);
  });

  it("renders the task outlet hook the scroll lock targets", () => {
    expect(appSource).toMatch(
      /route\.kind === "task" && route\.handle[\s\S]*?<TaskWorkspaceRoute/,
    );
    // `.task-detail` is the element the scroll lock targets; the terminal
    // region is a different node and would not prove this contract.
    render(<TaskTerminalView detail={detail()} />);
    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
  });

  it("fires onBack from the back control after the commit animation", async () => {
    vi.useFakeTimers();
    const onBack = vi.fn();
    render(<TaskTerminalView detail={detail()} onBack={onBack} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.click(screen.getByText("← Back"));
    expect(onBack).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(setSwipeEnterDirection).toHaveBeenCalledWith("right");
    expect(onBack).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("does not double-navigate when Back is clicked during settle", async () => {
    vi.useFakeTimers();
    const onBack = vi.fn();
    render(<TaskTerminalView detail={detail()} onBack={onBack} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.click(screen.getByText("← Back"));
    fireEvent.click(screen.getByText("← Back"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onBack).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("opens Diff Review on a left swipe", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    render(<TaskTerminalView detail={detail()} onOpenDiff={onOpenDiff} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.touchStart(root, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    fireEvent.touchMove(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("does not open Diff Review on a right swipe", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    const onBack = vi.fn();
    render(<TaskTerminalView detail={detail()} onOpenDiff={onOpenDiff} onBack={onBack} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.touchStart(root, { changedTouches: [{ clientX: 40, clientY: 40 }] });
    fireEvent.touchMove(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onBack).toHaveBeenCalledOnce();
    expect(onOpenDiff).not.toHaveBeenCalled();
    // Commit leaves the page translated off-screen until the route unmounts.
    expect(root.style.transform).toContain("390px");
    vi.useRealTimers();
  });

  it("opens Diff Review on a left swipe that begins on the terminal panel", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    render(<TaskTerminalView detail={detail()} onOpenDiff={onOpenDiff} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    const terminal = document.createElement("div");
    terminal.setAttribute("data-testid", "task-terminal-panel");
    root.appendChild(terminal);
    fireEvent.touchStart(terminal, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    fireEvent.touchMove(terminal, { changedTouches: [{ clientX: 120, clientY: 40 }] });
    fireEvent.touchEnd(terminal, { changedTouches: [{ clientX: 120, clientY: 40 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("keeps an in-flight left swipe when onOpenDiff identity changes", async () => {
    vi.useFakeTimers();
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(<TaskTerminalView detail={detail()} onOpenDiff={first} />);
    const root = screen.getByTestId("task-detail");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.touchStart(root, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    rerender(<TaskTerminalView detail={detail()} onOpenDiff={second} />);
    fireEvent.touchMove(root, { changedTouches: [{ clientX: 120, clientY: 40 }] });
    fireEvent.touchEnd(root, { changedTouches: [{ clientX: 120, clientY: 40 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(second).toHaveBeenCalledOnce();
    expect(first).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("does not own document scroll via ajax-task-open", () => {
    expect(taskTerminalViewSource).not.toMatch(/ajax-task-open/);
    expect(routeScrollSource).toMatch(/data-testid="route-scroll"/);
  });

  it("does not toggle document classes on mount", () => {
    document.documentElement.classList.remove("ajax-task-open");
    const { unmount } = render(<TaskTerminalView detail={detail()} />);

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);

    unmount();

    expect(document.documentElement.classList.contains("ajax-task-open")).toBe(false);
  });
});

describe("TaskTerminalView projection surface", () => {
  it("surfaces the runtime observation error as a warning", () => {
    render(
      <TaskTerminalView detail={detail({ runtime_observation_error: "tmux capture failed" })} />,
    );
    expect(screen.getByTestId("observation-error").textContent).toContain("tmux capture failed");
  });

  it("omits the observation warning when observation succeeded", () => {
    render(<TaskTerminalView detail={detail()} />);
    expect(screen.queryByTestId("observation-error")).not.toBeInTheDocument();
  });

  it("shows agent activity when it adds information beyond the status line", () => {
    render(
      <TaskTerminalView detail={detail({ agent_activity: "running cargo nextest" })} />,
    );
    expect(screen.getByTestId("agent-activity").textContent).toContain("running cargo nextest");
  });

  it("hides agent activity when it just repeats the status explanation", () => {
    render(
      <TaskTerminalView
        detail={detail({ agent_activity: "Ready for review", status_explanation: "Ready for review" })}
      />,
    );
    expect(screen.queryByTestId("agent-activity")).not.toBeInTheDocument();
  });

  it("falls back to the live status summary for the activity line", () => {
    render(
      <TaskTerminalView detail={detail({ agent_activity: null, live_status_summary: "waiting on approval" })} />,
    );
    expect(screen.getByTestId("agent-activity").textContent).toContain("waiting on approval");
  });

  it("exposes a footer Task details affordance wired to the workspace sheet", () => {
    render(<TaskTerminalView detail={detail()} onOpenDetails={vi.fn()} detailsPanelId="task-panel" />);
    const trigger = screen.getByTestId("task-meta-details-trigger");
    expect(trigger).toHaveTextContent("Task details");
    expect(trigger).toHaveAttribute("aria-controls", "task-panel");
    expect(screen.queryByTestId("task-meta-details-embedded")).not.toBeInTheDocument();
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

    expect(mobileBlock).toMatch(/\.task-meta-chrome\s*\{[^}]*margin-top:\s*0/);
  });

  it("keyboard-open hides footer meta chrome so hotkeys flush to the band bottom", () => {
    expect(stylesSource).toMatch(
      /html\.keyboard-open\s+\.task-detail\s+\.task-meta-chrome\s*\{[^}]*display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.task-meta-chrome[\s\S]*?display:\s*none/,
    );
  });

  it("marks footer meta-details inert during phone fullscreen expand", () => {
    const inertBody =
      taskTerminalSource.match(
        /const applyExpandedInert\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n {2}\};/,
      )?.[1] ?? "";

    expect(inertBody).toMatch(/querySelectorAll<HTMLElement>\(["']\.meta-details["']\)/);
    expect(inertBody).toMatch(/el\.inert\s*=\s*true/);
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
    // Horizontal pad must clear half the stadium min-height (~14px) or
    // "Tap to confirm" clips inside the rounded caps.
    expect(mobileBlock).toMatch(
      /\.interact-panel\s+\.action[\s\S]*?padding:\s*[0-4]px\s+(?:1[4-9]|[2-9]\d)px/,
    );
    expect(mobileBlock).toMatch(/\.interact-panel\s+\.action[\s\S]*?white-space:\s*nowrap/);
  });

  it("releases the mobile fill pin and caps the terminal when the task details sheet is open", () => {
    expect(stylesSource).toMatch(
      /\.task-detail\[data-task-details-open\][\s\S]*?flex:\s*0\s+0\s+auto/,
    );
    expect(stylesSource).toMatch(
      /\.task-detail\[data-task-details-open\][\s\S]*?min-height:\s*auto/,
    );

    const openMetaWrap = stylesSource.match(
      /\.task-detail\[data-task-details-open\]\s+\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap\s*\{([^}]*)\}/,
    );
    expect(openMetaWrap).not.toBeNull();
    const wrapBody = openMetaWrap![1];
    expect(wrapBody).toMatch(/min-height:\s*120px/);
    expect(wrapBody).toMatch(/(?:height|max-height):/);
  });

  it("bounds session details sheets with a contained scroller for iOS-safe reachability", () => {
    const scrimBlock =
      stylesSource.match(/\.session-sheet-scrim\s*\{([^}]*)\}/)?.[1] ?? "";
    const sheetBlock =
      stylesSource.match(/\.session-details-sheet\s*\{([^}]*)\}/)?.[1] ?? "";
    const bodyBlock =
      stylesSource.match(/\.session-details-body\s*\{([^}]*)\}/)?.[1] ?? "";
    const modelPickerBlock =
      stylesSource.match(/\.session-model-catalog \.model-picker\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(scrimBlock).toMatch(/flex-direction:\s*column/);
    expect(scrimBlock).toMatch(/justify-content:\s*flex-end/);
    expect(scrimBlock).toMatch(/overflow:\s*hidden/);
    expect(scrimBlock).toMatch(/min-height:\s*0/);
    expect(sheetBlock).toMatch(/flex:\s*0\s+1\s+auto/);
    expect(sheetBlock).toMatch(/max-height:\s*100%/);
    expect(sheetBlock).not.toMatch(/max-height:\s*calc\(100% - 24px\)/);
    expect(sheetBlock).toMatch(/env\(safe-area-inset-top/);
    expect(sheetBlock).toMatch(/env\(safe-area-inset-bottom/);
    expect(sheetBlock).toMatch(/overflow:\s*hidden/);
    expect(bodyBlock).toMatch(/flex:\s*1\s+1\s+auto/);
    expect(bodyBlock).toMatch(/min-height:\s*0/);
    expect(bodyBlock).toMatch(/overflow-y:\s*auto/);
    expect(modelPickerBlock).toMatch(/max-height:\s*46vh/);
    expect(modelPickerBlock).toMatch(/overflow-y:\s*auto/);
    expect(modelPickerBlock).toMatch(/overscroll-behavior:\s*contain/);
    expect(modelPickerBlock).toMatch(/-webkit-overflow-scrolling:\s*touch/);
    expect(modelPickerBlock).not.toMatch(/pointer-events:\s*none/); // #1022
  });

  it("keeps Details reachable in terminal-expanded fullscreen without hiding the control", () => {
    expect(stylesSource).not.toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s*\{[^}]*display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header[\s\S]*?pointer-events:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s+\.detail-header-controls[\s\S]*?pointer-events:\s*auto/,
    );

    const expandedDetailsCss =
      stylesSource.match(
        /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s+\.session-head-details\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(expandedDetailsCss).toMatch(/background:\s*var\(--paper-raised\)/);
    expect(expandedDetailsCss).toMatch(/color:\s*var\(--ink\)/);
    expect(expandedDetailsCss).toMatch(/min-height:\s*44px/);
    expect(expandedDetailsCss).toMatch(/border:\s*1px solid var\(--rule-strong\)/);
    expect(expandedDetailsCss).toMatch(/border-radius:\s*999px/);
  });
});
