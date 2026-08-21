import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act, waitFor, within } from "@testing-library/react";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import * as api from "@/shared/lib/api";
import * as useChatSpeechModule from "@/features/chat/speech/useChatSpeech";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import {
  ChatWithSheet,
  chatH,
  mountChat,
  openModelSwitchSheet,
  openSwitchPanel,
  openTaskDetails,
  prepareChatSurface,
  emitConnectedSnapshot,
  send,
  stylesSource,
  transport,
  typeComposer,
} from "./ChatSurface.testHarness";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("ChatSurface smoke", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  it("keeps replayed chat history when the session becomes ready", () => {
    chatH.autoReady = false;
    mountChat();
    send({ type: "message", role: "user", text: "Prior question", itemId: "u1" });
    send({ type: "message", role: "agent", text: "Prior answer", itemId: "a1" });
    send({ type: "turn_end", stopReason: "end_turn" });

    act(() => chatH.ready?.("auto"));

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

  it("does not blur the composer when tapping Mic, model, or Send", () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [{ value: "composer-2.5", name: "Composer 2.5" }],
      },
    ]);
    const composer = screen.getByLabelText("Message");
    composer.focus();
    fireEvent.pointerDown(screen.getByRole("button", { name: /Choose model/i }));
    expect(composer).toHaveFocus();
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
    vi.spyOn(useChatSpeechModule, "useChatSpeech").mockReturnValue({
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
    vi.spyOn(useChatSpeechModule, "useChatSpeech").mockReturnValue({
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
        // Replay carries the turn boundary too; a settled answer is what the
        // conversation shows, not a paragraph-gated live tail.
        callbacks.onEvent({ type: "turn_end", stopReason: "end_turn" });
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

    // Queueing a follow-up is not ACP activity: the agent has still gone quiet.
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "One more thing" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("No recent activity");
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));

    send({ type: "message", role: "thought", text: "Checking files", itemId: "t1" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("Working");
    expect(screen.getByTestId("session-head-thought")).toHaveTextContent("Checking files");
    send({ type: "turn_end" });
    expect(screen.getByTestId("session-head")).toHaveTextContent("Ready");
    expect(screen.queryByTestId("session-head-activity-age")).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it("queues one editable follow-up instead of sending it into a live turn", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");

    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(transport.sendCancel).not.toHaveBeenCalled();
    const queued = screen.getByTestId("session-queued");
    expect(queued).toHaveTextContent("Queued");
    expect(queued).toHaveTextContent("Next");
    expect(queued).toHaveTextContent("Press Enter again to stop and send now");
    expect(screen.getByRole("button", { name: "Stop & send" })).toBeInTheDocument();
  });

  it("sends the queued follow-up by itself when the turn ends normally", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    send({ type: "turn_end", stopReason: "end_turn" });

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next");
    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("session-message-user").at(-1)).toHaveTextContent("Next");
  });

  // The cancelled prompt and the follow-up must never be in flight together, so
  // the send waits for the host to resolve the turn it just cancelled.
  it("stops the turn on a second Enter and only then sends the follow-up", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });

    expect(transport.sendCancel).toHaveBeenCalledOnce();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Stopping…");

    send({ type: "turn_end", stopReason: "cancelled" });

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next");
    expect(screen.getByTestId("session-note-info")).toHaveTextContent("Stopped");
  });

  it("lets the operator edit or drop the queued follow-up", () => {
    mountChat();

    typeComposer("First");
    typeComposer("Next");
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByLabelText("Message")).toHaveValue("Next");
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();

    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();

    send({ type: "turn_end", stopReason: "end_turn" });
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("First");
  });

  it("names what Enter will do next on the composer action", () => {
    mountChat();

    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
    typeComposer("First");
    expect(screen.getByRole("button", { name: "Queue" })).toBeInTheDocument();
    typeComposer("Next");
    expect(screen.getByRole("button", { name: "Stop & send" })).toBeInTheDocument();
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
    const sheet = screen.getByTestId("task-details-sheet");
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

  it("#998 session route-scroll owns safe-area top because chat omits cockpit chrome", () => {
    const routeScrollBlock =
      stylesSource.match(
        /\[data-testid="route-scroll"\]:has\(\[data-outlet="session"\]\)\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(routeScrollBlock).toMatch(
      /padding:\s*env\(safe-area-inset-top\)\s+env\(safe-area-inset-right\)\s+0\s+env\(safe-area-inset-left\)/,
    );
  });

  it("#976 pins Ajax terminal outside the scrolling body with iOS-safe sheet insets", () => {
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
    expect(sheetBlock).toMatch(/flex:\s*0\s+1\s+auto/);
    expect(sheetBlock).toMatch(/env\(safe-area-inset-top/);
    expect(sheetBlock).not.toMatch(/max-height:\s*calc\(100% - 24px\)/);
    expect(bodyBlock).toMatch(/overflow-y:\s*auto/);
    expect(modelPickerBlock).toMatch(/max-height:\s*46vh/);
    expect(modelPickerBlock).toMatch(/overflow-y:\s*auto/);
    expect(modelPickerBlock).toMatch(/pointer-events:\s*none/);

    mountChat();
    openTaskDetails();
    const sheet = screen.getByTestId("task-details-sheet");
    const primaryTools = within(sheet).getByTestId("session-primary-tools");
    const body = within(sheet).getByTestId("session-details-body");
    const identity = within(sheet).getByTestId("session-task-identity");
    expect(body).not.toContainElement(primaryTools);
    expect(primaryTools).toHaveClass("session-sheet-tools-primary");
    expect(primaryTools.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(body).toContainElement(identity);
  });

  it("keeps Switch collapsed until opened (#979)", async () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [
          { value: "composer-2.5", name: "Composer 2.5" },
          { value: "auto", name: "Auto" },
        ],
      },
    ]);
    openTaskDetails();

    expect(screen.getByTestId("harness-swap")).not.toHaveClass("is-open");
    expect(screen.queryByTestId("harness-swap-harness-only")).not.toBeInTheDocument();

    openSwitchPanel();
    expect(await screen.findByTestId("harness-swap-harness-only")).toBeInTheDocument();
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
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
    expect(screen.getByTestId("harness-swap")).toBeInTheDocument();
    expect(screen.queryByText("Agent")).not.toBeInTheDocument();
  });

  it("hides the harness switch in the task details modal when the task has no agent", () => {
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "" } });
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
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
    expect(screen.getByTestId("task-details-sheet")).toContainElement(
      screen.getByTestId("test-in-dev"),
    );
  });

  it("hides Test in Dev in the task details modal for non-ajax-cli tasks", () => {
    mountChat();
    expect(screen.queryByTestId("test-in-dev")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
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
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();

    rerender(<ChatWithSheet pendingConfirmAction="drop" />);
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
  });

  // Connected Switch is harness-only; model changes use the model switch sheet.
  it("shows harness-only Switch when sessionConfigOptions are live (#936, #979)", async () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [
          { value: "composer-2.5", name: "Composer 2.5" },
          { value: "auto", name: "Auto" },
        ],
      },
    ]);
    openTaskDetails();
    openSwitchPanel();
    expect(await screen.findByTestId("harness-swap-harness-only")).toBeInTheDocument();
    expect(screen.queryByTestId("model-picker")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-config-pickers")).not.toBeInTheDocument();
    expect(transport.setConfigOption).not.toHaveBeenCalled();
  });

  it("keeps live model controls out of the composer footer", async () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [
          { value: "composer-2.5", name: "Composer 2.5" },
          { value: "auto", name: "Auto" },
        ],
      },
    ]);

    expect(screen.queryByTestId("session-config-pickers")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Choose model/i })).toBeEnabled();
    openModelSwitchSheet();
    expect(await screen.findByTestId("model-switch-sheet")).toBeInTheDocument();
    expect(screen.getByTestId("session-config-pickers")).toBeInTheDocument();
  });

  it("calls transport.setConfigOption when picking a Cursor model from the model sheet", async () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("grok-4.6", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "grok-4.6",
        choices: [
          { value: "grok-4.6", name: "Grok 4.6" },
          { value: "composer-2.5", name: "Composer 2.5" },
        ],
      },
      {
        id: "reasoning",
        category: "thought_level",
        name: "Effort",
        type: "select",
        currentValue: "xhigh",
        choices: [
          { value: "xhigh", name: "Extra High" },
          { value: "high", name: "High" },
        ],
      },
    ]);

    openModelSwitchSheet();
    fireEvent.click(
      within(await screen.findByTestId("session-config-model")).getByRole("radio", {
        name: "Composer 2.5",
      }),
    );
    expect(transport.setConfigOption).toHaveBeenCalledWith("model", "composer-2.5");
    expect(transport.setModel).not.toHaveBeenCalled();
  });

  it("disables the model hotbar control until the session connects", () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    expect(screen.queryByRole("button", { name: /Choose model/i })).not.toBeInTheDocument();

    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [{ value: "composer-2.5", name: "Composer 2.5" }],
      },
    ]);
    expect(screen.getByRole("button", { name: /Choose model/i })).toBeEnabled();
  });
});
