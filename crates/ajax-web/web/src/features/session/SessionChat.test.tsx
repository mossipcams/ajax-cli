import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, screen, act, waitFor, within } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import SessionChat from "./SessionChat";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import * as useTaskTerminalSpeechModule from "@/features/task/useTaskTerminalSpeech";
import * as api from "@/shared/lib/api";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { writeSessionModel } from "./sessionModel";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../.."));

const transport = {
  // `WebSessionTransport.sendPrompt` returns the clientMessageId it queued, and
  // "" when it refuses to send; the composer keys off that.
  sendPrompt: vi.fn(() => "cmid-1"),
  sendCancel: vi.fn(),
  setModel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

let emit: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
let ready: ((model: string) => void) | undefined;
let autoReady = true;
let frameQueue: FrameRequestCallback[] = [];

function flushRaf() {
  act(() => {
    for (const callback of frameQueue.splice(0)) callback(0);
  });
}

function stubSessionTransport() {
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, callbacks) => {
      emit = callbacks.onEvent;
      ready = callbacks.onReady;
      if (autoReady) callbacks.onReady("auto");
      return transport;
    },
  );
}

function openTaskDetails() {
  fireEvent.click(screen.getByTestId("session-details"));
}

function openModelCatalog() {
  fireEvent.click(screen.getByTestId("session-model-change"));
}

function mountChat(overrides: Partial<React.ComponentProps<typeof SessionChat>> = {}) {
  return render(
    <SessionChat
      handle="web/fix-login"
      detail={taskDetail as BrowserTaskDetail}
      detailStatus="ready"
      {...overrides}
    />,
  );
}

function send(event: webSessionTransport.WebSessionServerEvent) {
  act(() => emit?.(event));
  flushRaf();
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("SessionChat smoke", () => {
  beforeEach(() => {
    emit = undefined;
    ready = undefined;
    autoReady = true;
    frameQueue = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frameQueue.push(callback);
      return frameQueue.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    transport.sendPrompt.mockClear();
    transport.setModel.mockClear();
    transport.respondPermission.mockClear();
    localStorage.clear();
    sessionStorage.clear();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
        }),
      }),
    );
    stubSessionTransport();
  });

  it("keeps replayed chat history when the session becomes ready", () => {
    autoReady = false;
    mountChat();
    send({ type: "message", role: "user", text: "Prior question", itemId: "u1" });
    send({ type: "message", role: "agent", text: "Prior answer", itemId: "a1" });

    act(() => ready?.("auto"));

    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Prior question");
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Prior answer");
  });

  it("leads with the live head", () => {
    mountChat();
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start voice input" })).toBeInTheDocument();
  });

  it("shows ACP context usage in the head while idle and below 70%", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" },
    });
    send({ type: "usage", used: 30, size: 100 });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "idle");
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 30% full");
  });

  it("does not render context usage for a zero-size update", () => {
    mountChat();
    send({ type: "usage", used: 0, size: 0 });
    expect(screen.queryByTestId("session-usage")).not.toBeInTheDocument();
  });

  it("stores per-turn token usage without showing it in the head", () => {
    mountChat();
    send({ type: "turn_usage", inputTokens: 1200, outputTokens: 300, totalTokens: 1500 });
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
  });

  it("does not render turn usage when turn_usage carries no token counts", () => {
    mountChat();
    send({ type: "turn_usage", requestId: "req-only" });
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
  });

  it("shows context usage without a turn token line", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" },
    });
    send({ type: "usage", used: 40, size: 100 });
    send({ type: "turn_usage", inputTokens: 900, totalTokens: 900 });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 40% full");
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
  });

  // Regression for #877: sticky inside the masked overflow scroller left the
  // composer stranded mid-viewport after the iOS keyboard dismissed.
  it("docks the composer below the transcript scroller, not inside it", () => {
    mountChat();
    const chat = screen.getByTestId("session-chat");
    const surface = screen.getByTestId("session-chat-surface");
    const thread = screen.getByTestId("session-thread");
    const composer = screen.getByTestId("session-composer");
    expect(thread).not.toContainElement(composer);
    expect(chat).toContainElement(composer);
    expect(surface).toContainElement(thread);
    expect(surface).toContainElement(composer);
  });

  it("blurs the composer when tapping the transcript scroller", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    fireEvent.pointerDown(screen.getByTestId("session-thread"));
    expect(composer).not.toHaveFocus();
  });

  it("blurs the composer when tapping dead space on the session page below the thread", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    fireEvent.pointerDown(screen.getByTestId("session-chat"));
    expect(composer).not.toHaveFocus();
  });

  it("does not blur the composer when tapping Mic or Send", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Send" }));
    expect(composer).toHaveFocus();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Start voice input" }));
    expect(composer).toHaveFocus();
  });

  it("styles the idle mic as accent text only, not a pill", () => {
    const micCss =
      stylesSource.match(/\.session-composer-mic\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(micCss).toMatch(/border-radius:\s*0/);
    expect(micCss).not.toMatch(/border-radius:\s*999px/);
    expect(micCss).toMatch(/background:\s*transparent/);
    expect(micCss).toMatch(/border:\s*none/);
    expect(micCss).toMatch(/color:\s*var\(--accent\)/);
    expect(micCss).not.toMatch(/background:\s*var\(--accent\)/);

    const sendCss =
      stylesSource.match(/\.session-composer-send\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(sendCss).toMatch(/background:\s*var\(--accent\)/);
    expect(stylesSource).toMatch(
      /\.session-composer-button\s*\{[\s\S]*?border-radius:\s*999px/,
    );
  });

  it("shows warn text while listening without a filled chip", () => {
    vi.spyOn(useTaskTerminalSpeechModule, "useTaskTerminalSpeech").mockReturnValue({
      speechModel: {
        state: "listening",
        sessionId: "speech-session-test",
        errorMessage: null,
        finalTranscript: "",
        partialTranscript: "",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
      },
      pauseCountdownSeconds: undefined,
      micAriaLabel: "Stop voice input",
      micArmed: true,
      toggleMic: vi.fn(),
      cancelSpeechInput: vi.fn(),
      cancelSpeechTransport: vi.fn(),
    });
    mountChat();
    const mic = screen.getByRole("button", { name: "Stop voice input" });
    expect(mic).toHaveClass("is-armed");
    expect(mic).toHaveTextContent("Mic");
  });

  it("shows warn text while connecting without looking disabled-dead", () => {
    vi.spyOn(useTaskTerminalSpeechModule, "useTaskTerminalSpeech").mockReturnValue({
      speechModel: {
        state: "connecting",
        sessionId: "speech-session-test",
        errorMessage: null,
        finalTranscript: "",
        partialTranscript: "",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
      },
      pauseCountdownSeconds: undefined,
      micAriaLabel: "Start voice input",
      micArmed: false,
      toggleMic: vi.fn(),
      cancelSpeechInput: vi.fn(),
      cancelSpeechTransport: vi.fn(),
    });
    mountChat();
    const mic = screen.getByRole("button", { name: "Start voice input" });
    expect(mic).toHaveClass("is-connecting");
    expect(mic).toBeDisabled();
    expect(mic).toHaveTextContent("Mic");
  });

  it("keeps transcript events replayed before ready", () => {
    vi.restoreAllMocks();
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, callbacks) => {
        callbacks.onEvent({
          type: "message",
          role: "agent",
          text: "Earlier reply",
          itemId: "a1",
        });
        callbacks.onReady("auto");
        return transport;
      },
    );

    mountChat();

    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Earlier reply");
  });

  it("sends composer messages through ACP on Enter", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test");
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
    send({ type: "message", role: "user", text: "Please fix the flaky test", itemId: "u1" });
    expect(screen.getAllByTestId("session-message-user")).toHaveLength(1);
  });

  it("surfaces a permission request in the head with Approve and Reject", () => {
    mountChat();
    send({
      type: "permission_request",
      requestId: "7",
      title: "Run cargo test?",
      detail: "cargo test -p ajax-web",
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "decision");
    expect(screen.getByTestId("session-decision")).toHaveTextContent("Run cargo test?");
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("7", true);
    expect(screen.getByTestId("session-decision")).toBeInTheDocument();
    send({ type: "permission_resolved", requestId: "7", approved: true });
    expect(screen.queryByTestId("session-decision")).not.toBeInTheDocument();
  });

  // Regression for #889: stable ACP v1 has no stalled signal, so expose event
  // freshness without changing the host-owned in-flight state.
  it("shows when a busy turn has stopped producing ACP activity", async () => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
    vi.setSystemTime(new Date("2026-08-15T12:00:00Z"));
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" },
    });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Keep going" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });

    expect(screen.getByTestId("session-head")).toHaveTextContent("Working");
    await act(async () => vi.advanceTimersByTimeAsync(60_000));
    expect(screen.getByTestId("session-head")).toHaveTextContent("No recent activity");
    expect(screen.getByTestId("session-head-activity-age")).toHaveTextContent(
      "Last update 1m ago",
    );

    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "One more thing" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("No recent activity");

    send({ type: "message", role: "thought", text: "Checking files", itemId: "t1" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("Working");
    expect(screen.getByTestId("session-head-thought")).toHaveTextContent("Checking files");
    send({ type: "turn_end" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("Ready");
    expect(screen.queryByTestId("session-head-activity-age")).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it("sends one host-queued prompt while busy without a browser follow-up latch", () => {
    mountChat();

    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "First" } });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendPrompt.mockClear();

    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "Next" } });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next");
    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Sends after this turn…",
    );
  });

  it("opens Diff Review on a left swipe", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    mountChat({ onOpenDiff });
    const root = screen.getByTestId("session-chat");
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

  it("calls onOpenTerminal from the Ajax terminal control in task details", () => {
    const onOpenTerminal = vi.fn();
    mountChat({ onOpenTerminal });
    openTaskDetails();
    fireEvent.click(screen.getByTestId("session-ajax-terminal"));
    expect(onOpenTerminal).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("session-terminal-sheet")).not.toBeInTheDocument();
  });

  it("leads the task details sheet with task identity (#p1 layout)", () => {
    mountChat();
    openTaskDetails();
    const sheet = screen.getByTestId("session-task-panel");
    const identity = screen.getByTestId("session-task-identity");
    const terminal = screen.getByTestId("session-ajax-terminal");
    const meta = screen.getByTestId("task-meta-details-embedded");
    expect(identity).toHaveTextContent("Fix login");
    expect(identity).toHaveTextContent("web/fix-login");
    expect(identity).toHaveTextContent("ajax/fix-login");
    expect(terminal.compareDocumentPosition(identity) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(identity.compareDocumentPosition(meta) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(sheet).getByTestId("session-details-body")).toContainElement(identity);
  });

  it("#976 pins Ajax terminal outside the scrolling body with iOS-safe sheet insets", () => {
    const scrimBlock =
      stylesSource.match(/\.session-sheet-scrim\s*\{([^}]*)\}/)?.[1] ?? "";
    const sheetBlock =
      stylesSource.match(/\.session-details-sheet\s*\{([^}]*)\}/)?.[1] ?? "";
    const bodyBlock =
      stylesSource.match(/\.session-details-body\s*\{([^}]*)\}/)?.[1] ?? "";
    const modelPickerBlock =
      stylesSource.match(
        /\.session-details-sheet \.session-model-catalog \.model-picker[\s\S]*?\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(scrimBlock).toMatch(/flex-direction:\s*column/);
    expect(scrimBlock).toMatch(/justify-content:\s*flex-end/);
    expect(scrimBlock).toMatch(/overflow:\s*hidden/);
    expect(sheetBlock).toMatch(/flex:\s*0\s+1\s+auto/);
    expect(sheetBlock).toMatch(/env\(safe-area-inset-top/);
    expect(sheetBlock).not.toMatch(/max-height:\s*calc\(100% - 24px\)/);
    expect(bodyBlock).toMatch(/overflow-y:\s*auto/);
    expect(modelPickerBlock).toMatch(/max-height:\s*none/);
    expect(modelPickerBlock).toMatch(/overflow:\s*visible/);

    mountChat();
    openTaskDetails();
    const sheet = screen.getByTestId("session-task-panel");
    const primaryTools = within(sheet).getByTestId("session-primary-tools");
    const body = within(sheet).getByTestId("session-details-body");
    const identity = within(sheet).getByTestId("session-task-identity");
    expect(body).not.toContainElement(primaryTools);
    expect(primaryTools).toHaveClass("session-sheet-tools-primary");
    expect(primaryTools.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(body).toContainElement(identity);
  });

  it("keeps the model catalog collapsed until Change is opened (#p1 distill)", async () => {
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    act(() => ready?.("composer-2.5"));
    openTaskDetails();

    expect(screen.getByTestId("session-model-summary")).toBeInTheDocument();
    expect(screen.queryByTestId("session-model-catalog")).not.toBeInTheDocument();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByTestId("session-model-current")).toHaveTextContent(/Composer 2\.5/i);
    });

    openModelCatalog();
    expect(await screen.findByRole("radio", { name: /Composer 2\.5/i })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("does not render Rust Debug annotation strings in task details (#p1 clarify)", () => {
    mountChat();
    openTaskDetails();
    expect(screen.queryByText(/Annotation\s*\{/)).not.toBeInTheDocument();
    const notes = screen.getByTestId("task-annotations");
    expect(notes).toHaveTextContent("waiting for approval");
    expect(notes).toHaveTextContent("reviewable · reviewable");
  });

  it("shows the harness switch in the task details modal when the task has an agent", () => {
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    expect(screen.queryByTestId("harness-swap")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.getByTestId("harness-swap")).toBeInTheDocument();
    expect(screen.queryByText("Agent")).not.toBeInTheDocument();
  });

  it("hides the harness switch in the task details modal when the task has no agent", () => {
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "" } });
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("harness-swap")).not.toBeInTheDocument();
  });

  it("shows Test in Dev in the task details modal for ajax-cli tasks", async () => {
    vi.spyOn(api, "fetchDevDeploy").mockResolvedValue({
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
    mountChat({
      detail: {
        ...(taskDetail as BrowserTaskDetail),
        repo: "ajax-cli",
        qualified_handle: "ajax-cli/demo",
      },
    });
    expect(screen.queryByTestId("test-in-dev")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    await waitFor(() => {
      expect(screen.getByTestId("test-in-dev")).toBeInTheDocument();
    });
    expect(screen.getByTestId("session-task-panel")).toContainElement(
      screen.getByTestId("test-in-dev"),
    );
  });

  it("hides Test in Dev in the task details modal for non-ajax-cli tasks", () => {
    mountChat();
    expect(screen.queryByTestId("test-in-dev")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("test-in-dev")).not.toBeInTheDocument();
  });

  it("calls onSwappedAgent and onMutated once after a successful harness swap in the details modal", async () => {
    vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    const onSwappedAgent = vi.fn();
    const onMutated = vi.fn();
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" },
      onSwappedAgent,
      onMutated,
    });
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(onSwappedAgent).toHaveBeenCalledOnce());
    expect(onMutated).toHaveBeenCalledOnce();
  });

  // Regression for #930: after keyboard dismiss the one-shot restore can leave
  // scrollTop lagging scrollHeight; pinned readers must follow new content.
  it("follows the live edge on new items while pinned", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "First", itemId: "a1" });
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    thread.scrollTop = 800;
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1400 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });

    send({ type: "message", role: "agent", text: "Streaming chunk", itemId: "a2" });
    expect(thread.scrollTop).toBe(thread.scrollHeight);
  });

  it("does not yank history readers when transcript grows", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "First", itemId: "a1" });
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 2000 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
    thread.scrollTop = 120;
    fireEvent.scroll(thread);

    const before = thread.scrollTop;
    send({ type: "message", role: "agent", text: "Second", itemId: "a2" });
    expect(thread.scrollTop).toBe(before);
    expect(thread.scrollTop).not.toBe(thread.scrollHeight);
  });

  it("follows the live edge on thread resize while pinned", () => {
    const resizeCallbacks: ResizeObserverCallback[] = [];
    class MockResizeObserver {
      private readonly callback: ResizeObserverCallback;
      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
        resizeCallbacks.push(callback);
      }
      observe() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", MockResizeObserver);

    mountChat();
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    thread.scrollTop = 800;
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1400 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });

    act(() => {
      resizeCallbacks.at(-1)?.(
        [{ target: thread } as ResizeObserverEntry],
        {} as ResizeObserver,
      );
    });

    expect(thread.scrollTop).toBe(1400);
  });

  it("follows the live edge on transcript DOM mutations while pinned", () => {
    const mutationCallbacks: MutationCallback[] = [];
    class MockMutationObserver {
      private readonly callback: MutationCallback;
      constructor(callback: MutationCallback) {
        this.callback = callback;
        mutationCallbacks.push(callback);
      }
      observe() {}
      disconnect() {}
    }
    vi.stubGlobal("MutationObserver", MockMutationObserver);

    mountChat();
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    thread.scrollTop = 800;
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1500 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });

    act(() => {
      mutationCallbacks.at(-1)?.([], {} as MutationObserver);
    });

    expect(thread.scrollTop).toBe(1500);
  });

  // Regression for #947: Drop confirm arms in the shell ResultPanel; the details
  // sheet (FullscreenLayer z-index 50) must close so Confirm is reachable.
  it("closes the task details sheet when Drop confirm arms (#947)", () => {
    const { rerender } = mountChat({ pendingConfirmAction: null });
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();

    rerender(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
        pendingConfirmAction="drop"
      />,
    );
    expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();
  });

  // Regression for #936: native <select> in the Radix task-details sheet was not
  // operable, and composite session_model values did not show as selected.
  it("shows the live session model as selected and changes it from task details (#936)", async () => {
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    act(() => ready?.("composer-2.5"));
    openTaskDetails();
    openModelCatalog();

    const current = await screen.findByRole("radio", { name: /Composer 2\.5/i });
    expect(current).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: /Auto/i }));
    expect(transport.setModel).toHaveBeenCalledWith("auto");
  });

  it("shows a composite session model as selected in task details (#936)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [{ id: "opus", label: "Opus" }],
          default: "opus",
          reasoning: {
            id: "effort",
            label: "Effort",
            default: "medium",
            options: [
              { id: "low", label: "Low" },
              { id: "high", label: "High" },
            ],
          },
        }),
      }),
    );
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "claude" } });
    act(() => ready?.("opus|effort=high"));
    openTaskDetails();
    openModelCatalog();

    const current = await screen.findByRole("radio", { name: /Opus/i });
    expect(current).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "High" })).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(transport.setModel).toHaveBeenCalledWith("opus|effort=low");
  });

  // Regression for #948: task details show a shortlist first, with Show all for the rest.
  it("shows a model shortlist with Show all in task details (#948)", async () => {
    const catalog = {
      models: Array.from({ length: 12 }, (_, index) => ({
        id: `model-${index}`,
        label: `Catalog Model ${index}`,
      })),
      default: "model-0",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => catalog,
      }),
    );
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    act(() => ready?.("model-2"));
    openTaskDetails();
    openModelCatalog();

    await waitFor(() => {
      expect(screen.getByTestId("model-picker-toggle")).toHaveTextContent("Show all");
    });
    expect(screen.queryByRole("radio", { name: "Catalog Model 11" })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Catalog Model 2" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByTestId("model-picker-toggle")).toHaveTextContent("Show all");
    fireEvent.click(screen.getByTestId("model-picker-toggle"));
    expect(screen.getAllByRole("radio", { name: /Catalog Model/i })).toHaveLength(
      catalog.models.length,
    );
  });

  // Regression for #952: the in-session picker must reflect snapshot applied model,
  // not task metadata or localStorage, when the host reports a different harness id.
  it("shows the host snapshot applied model in task details (#952)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "harness-default", label: "Harness default" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
          default: "harness-default",
        }),
      }),
    );
    writeSessionModel("composer-2.5");
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    act(() => ready?.("harness-default"));

    openTaskDetails();
    openModelCatalog();
    const current = await screen.findByRole("radio", { name: /Harness default/i });
    expect(current).toHaveAttribute("aria-checked", "true");
    expect(
      screen.queryByRole("radio", { name: /Composer 2\.5/i, checked: true }),
    ).toBeNull();
  });
});

describe("SessionChat task details polish", () => {
  beforeEach(() => {
    emit = undefined;
    ready = undefined;
    autoReady = true;
    frameQueue = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frameQueue.push(callback);
      return frameQueue.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    transport.sendPrompt.mockClear();
    transport.setModel.mockClear();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
        }),
      }),
    );
    stubSessionTransport();
  });

  it("styles sheet field labels with tracked uppercase chrome", () => {
    const labelCss =
      stylesSource.match(/\.session-details-sheet \.field-label\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(labelCss).toMatch(/text-transform:\s*uppercase/);
    expect(labelCss).toMatch(/letter-spacing:\s*var\(--tracking-label\)/);
    expect(labelCss).toMatch(/color:\s*var\(--ink-muted\)/);
  });

  it("lifts Close to 44px in the task details sheet", () => {
    const closeCss =
      stylesSource.match(
        /\.session-details-sheet \.session-sheet-header \.pill[\s\S]*?\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(closeCss).toMatch(/min-height:\s*44px/);
  });

  it("exposes aria-expanded on Details and model Change", async () => {
    autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    act(() => ready?.("composer-2.5"));

    const details = screen.getByTestId("session-details");
    expect(details).toHaveAttribute("aria-expanded", "false");
    expect(details).toHaveAttribute("aria-controls");

    openTaskDetails();
    expect(details).toHaveAttribute("aria-expanded", "true");

    const change = screen.getByTestId("session-model-change");
    expect(change).toHaveAttribute("aria-expanded", "false");
    expect(change).toHaveAttribute("aria-controls");

    openModelCatalog();
    expect(await screen.findByTestId("session-model-catalog")).toHaveAttribute("id");
    expect(screen.getByTestId("session-model-done")).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByTestId("session-model-change")).not.toBeInTheDocument();
  });

  it("pins observation error under identity with the task-detail prefix", () => {
    mountChat({
      detail: {
        ...(taskDetail as BrowserTaskDetail),
        runtime_observation_error: "tmux session missing",
      },
    });
    openTaskDetails();

    const identity = screen.getByTestId("session-task-identity");
    const observationError = screen.getByTestId("session-observation-error");
    const modelSelect = screen.getByTestId("session-model-select");
    expect(observationError).toHaveTextContent("Observation error: tmux session missing");
    expect(identity.compareDocumentPosition(observationError) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(observationError.compareDocumentPosition(modelSelect) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("hides the session model picker while harness Switch is open", () => {
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    openTaskDetails();
    expect(screen.getByTestId("session-model-select")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(screen.queryByTestId("session-model-select")).not.toBeInTheDocument();
    expect(screen.getByTestId("harness-swap")).toHaveClass("is-open");
  });

  it("does not give the first sheet ActionBar action primary fill", () => {
    const mutedCss =
      stylesSource.match(/\.session-sheet-actions-muted \.action\.primary\s*\{([^}]*)\}/)?.[1] ??
      "";
    expect(mutedCss).toMatch(/background:\s*transparent/);
    expect(mutedCss).not.toMatch(/background:\s*var\(--accent\)/);
  });
});
