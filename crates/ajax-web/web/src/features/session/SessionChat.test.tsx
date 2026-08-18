import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, screen, act, waitFor } from "@testing-library/react";
import SessionChat from "./SessionChat";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import * as useTaskTerminalSpeechModule from "@/features/task/useTaskTerminalSpeech";
import * as api from "@/shared/lib/api";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readFileSync(join(here, "../../styles.css"), "utf8");

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
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(screen.getByTestId("session-ajax-terminal"));
    expect(onOpenTerminal).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("session-terminal-sheet")).not.toBeInTheDocument();
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
    fireEvent.click(screen.getByTestId("session-details"));

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
    fireEvent.click(screen.getByTestId("session-details"));

    const current = await screen.findByRole("radio", { name: /Opus/i });
    expect(current).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "High" })).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(transport.setModel).toHaveBeenCalledWith("opus|effort=low");
  });

  // Regression for #948: task details must list the full harness catalog, not Auto
  // plus the live session model when the API advertises more.
  it("lists the full harness catalog in task details (#948)", async () => {
    const catalog = {
      models: Array.from({ length: 8 }, (_, index) => ({
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
    fireEvent.click(screen.getByTestId("session-details"));

    await waitFor(() => {
      expect(screen.getAllByRole("radio", { name: /Catalog Model/i })).toHaveLength(
        catalog.models.length,
      );
    });
    expect(screen.getByRole("radio", { name: "Catalog Model 2" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});
