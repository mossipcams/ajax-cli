import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, fireEvent, screen, waitFor, act, within } from "@testing-library/react";
import SessionStarter from "./SessionStarter";
import SessionChat, { sessionSeededStorageKey } from "./SessionChat";
import * as api from "@/shared/lib/api";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const repos = [{ name: "web" }];

const transport = {
  sendPrompt: vi.fn(),
  sendCancel: vi.fn(),
  setModel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

let emit: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
let signalReady: ((model?: string) => void) | undefined;
let closeSocket: (() => void) | undefined;

function stubSessionTransport(options: { autoReady?: boolean } = { autoReady: true }) {
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, callbacks) => {
      emit = callbacks.onEvent;
      signalReady = (model = "auto") => callbacks.onReady(model);
      closeSocket = callbacks.onClosed;
      if (options.autoReady !== false) {
        callbacks.onReady("auto");
      }
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
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  Object.defineProperty(document, "visibilityState", {
    value: "visible",
    configurable: true,
  });
  localStorage.clear();
  sessionStorage.clear();
});

describe("SessionStarter", () => {
  beforeEach(() => {
    localStorage.clear();
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
  });

  it("submits cursor orchestration_chat startTask and reports handle with starter context", async () => {
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const onStarted = vi.fn();
    render(<SessionStarter repos={repos} onStarted={onStarted} />);
    expect(screen.getByTestId("session-agent-lock")).toHaveTextContent("Cursor");
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.change(screen.getByLabelText("Constraints"), {
      target: { value: "No API changes" },
    });
    fireEvent.change(screen.getByLabelText("Expected outcome"), {
      target: { value: "Green tests" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Start session" }));
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    expect(spy).toHaveBeenCalledWith(
      expect.objectContaining({
        agent: "cursor",
        orchestration_chat: true,
      }),
    );
    expect(onStarted).toHaveBeenCalledWith("web/fix-login", {
      title: "Fix login",
      constraints: "No API changes",
      expectedOutcome: "Green tests",
    });
  });

  it("marks the brief fields optional so only repo and title gate the start", () => {
    render(<SessionStarter repos={repos} />);
    expect(screen.getByText(/optional/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start session" })).toBeEnabled();
  });
});

describe("SessionChat", () => {
  beforeEach(() => {
    emit = undefined;
    signalReady = undefined;
    closeSocket = undefined;
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    transport.setModel.mockClear();
    transport.respondPermission.mockClear();
    transport.dispose.mockClear();
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

  it("leads with the live head and keeps task detail one tap away", () => {
    mountChat();
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("session-thread-empty")).toHaveTextContent(
      "Message the agent to steer this task.",
    );
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.getByTestId("session-artifact-status")).toHaveTextContent("Waiting for approval");
    expect(screen.getByTestId("session-artifact-activity")).toHaveTextContent("waiting for review");
    expect(screen.getByTestId("session-artifact-annotations")).toBeInTheDocument();
    expect(screen.getByTestId("session-quick-actions")).toBeInTheDocument();
  });

  it("shows a task that needs attention in the head without a separate banner", () => {
    mountChat();
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "attention");
    expect(screen.getByTestId("session-attention")).toHaveTextContent("Waiting for approval");
  });

  it("reports idle when the task is neither waiting nor in error", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" } as BrowserTaskDetail,
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "idle");
  });

  it("sends composer messages through ACP and echoes them in the transcript", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Session composer" }));
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test");
    // The host records the prompt in the shared transcript and streams it back.
    send({ type: "message", role: "user", text: "Please fix the flaky test" });
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
  });

  it("sends on Enter and keeps Shift+Enter as newline", () => {
    mountChat();
    const message = screen.getByLabelText("Message");
    fireEvent.change(message, { target: { value: "Ship it" } });
    fireEvent.keyDown(message, { key: "Enter", shiftKey: true });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship it");
  });

  it("sends one starter brief via ACP after ready", async () => {
    mountChat({
      starterContext: {
        title: "Fix login",
        constraints: "No API changes",
        expectedOutcome: "Green tests",
      },
    });
    await waitFor(() => expect(transport.sendPrompt).toHaveBeenCalledOnce());
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      "Fix login\n\nConstraints: No API changes\n\nDone when: Green tests",
    );
    expect(sessionStorage.getItem(sessionSeededStorageKey("web/fix-login"))).toBe("1");
    send({ type: "message", role: "user", text: "Fix login" });
    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Fix login");
  });

  it("does not re-send the starter brief after remount with the same handle", async () => {
    const starterContext = {
      title: "Fix login",
      constraints: "No API changes",
      expectedOutcome: "Green tests",
    };
    const { unmount } = mountChat({ starterContext });
    await waitFor(() => expect(transport.sendPrompt).toHaveBeenCalledOnce());
    unmount();
    mountChat({ starterContext });
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(transport.sendPrompt).toHaveBeenCalledOnce();
  });

  it("shows queued-send placeholder while a turn is in flight", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    expect(screen.getByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Sends after this turn…",
    );
  });

  it("shows stop-and-send placeholder after queuing a follow-up during a turn", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship the fix next");
    expect(screen.getByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Enter again to stop and send",
    );
  });

  it("empty Enter after a queued follow-up cancels in flight but keeps the queue", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship the fix next");
  });

  it("double Enter while busy queues once then stops on the second press", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    const message = screen.getByLabelText("Message");
    fireEvent.change(message, { target: { value: "Ship the fix next" } });
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
  });

  it("idle send then empty Enter does not cancel", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "Ship it" } });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendCancel).not.toHaveBeenCalled();
  });

  it("Stop clears the queue with a plain cancel", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.click(screen.getByTestId("session-cancel"));
    expect(transport.sendCancel).toHaveBeenCalledWith();
  });

  it("renders a tool call as a labelled row in the head, not a JSON dump", () => {
    mountChat();
    send({
      type: "tool_call",
      callId: "c1",
      title: "Read configuration",
      kind: "read",
      status: "in_progress",
      locations: ["/repo/crates/ajax-web/src/lib.rs"],
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "working");
    const tool = screen.getByTestId("session-head-tool");
    expect(tool).toHaveTextContent("read");
    expect(tool).toHaveTextContent("Read configuration");
    expect(tool).toHaveTextContent("…/src/lib.rs");
    expect(screen.queryByTestId("session-tools")).not.toBeInTheDocument();
    expect(screen.queryByText(/sessionUpdate/)).not.toBeInTheDocument();
  });

  it("summarizes a turn's tools into one transcript note", () => {
    mountChat();
    send({
      type: "tool_call",
      callId: "c1",
      title: "Read configuration",
      kind: "read",
      status: "completed",
      locations: ["/repo/a.ts"],
    });
    send({
      type: "tool_call",
      callId: "c2",
      title: "Edit web_session.rs",
      kind: "edit",
      status: "completed",
      locations: ["/repo/b.ts"],
    });
    expect(screen.queryByTestId("session-tools")).not.toBeInTheDocument();
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.getByTestId("session-note-info")).toHaveTextContent("1 read · 1 edit");
  });

  it("shows only the in-progress plan step in the head", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    send({
      type: "plan",
      entries: [
        { content: "Read", status: "completed" },
        { content: "Patch the router", status: "in_progress" },
        { content: "Cover both orders", status: "pending" },
      ],
    });
    expect(screen.getByTestId("session-plan-step")).toHaveTextContent("Patch the router");
    expect(screen.queryByTestId("session-plan")).not.toBeInTheDocument();
    expect(screen.queryByText("Cover both orders")).not.toBeInTheDocument();
  });

  it("renders agent markdown as real code and list elements", () => {
    mountChat();
    send({
      type: "message",
      role: "agent",
      text: "Fixed it:\n\n- ran `cargo test`\n\n```\nok\n```",
    });
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.getByRole("listitem")).toHaveTextContent("ran cargo test");
    expect(screen.getByText("cargo test").tagName).toBe("CODE");
    expect(screen.getByText("ok").tagName).toBe("CODE");
  });

  it("surfaces a permission request as the head decision with Approve and Reject", () => {
    mountChat();
    send({
      type: "permission_request",
      requestId: "42",
      title: "Run cargo test?",
      detail: "in crates/ajax-web",
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "decision");
    const decision = screen.getByTestId("session-decision");
    expect(decision).toHaveTextContent("Run cargo test?");
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("42", true);
    expect(screen.queryByTestId("session-decision")).not.toBeInTheDocument();
  });

  it("rejects a permission request without approving it", () => {
    mountChat();
    send({ type: "permission_request", requestId: "42", title: "Delete the branch?" });
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("42", false);
  });

  it("offers Stop only while a turn is in flight", () => {
    mountChat();
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.click(screen.getByTestId("session-cancel"));
    expect(transport.sendCancel).toHaveBeenCalledOnce();
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
  });

  it("keeps reasoning out of the transcript and the head", () => {
    mountChat();
    send({ type: "message", role: "thought", text: "Checking the router" });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "working");
    expect(screen.queryByTestId("session-thought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-message-agent")).not.toBeInTheDocument();
    send({ type: "message", role: "agent", text: "Found it" });
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Found it");
  });

  it("keeps run status out of the transcript", () => {
    mountChat();
    send({ type: "status", state: "running" });
    send({ type: "status", state: "running" });
    expect(screen.getByTestId("session-thread-empty")).toBeInTheDocument();
  });

  it("reports a dropped connection in the head", () => {
    mountChat();
    expect(screen.queryByTestId("session-head-offline")).not.toBeInTheDocument();
    act(() => closeSocket?.());
    expect(screen.getByTestId("session-head-offline")).toBeInTheDocument();
  });

  it("does not redial while hidden after a post-ready drop; visibilitychange redials", () => {
    vi.useFakeTimers();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    mountChat();
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledOnce();

    Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
    act(() => closeSocket?.());

    const dialsBefore = vi.mocked(webSessionTransport.connectWebSessionTransport).mock.calls.length;
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledTimes(dialsBefore);

    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledTimes(dialsBefore + 1);
    vi.useRealTimers();
  });

  it("keeps redialing after more than five post-ready visible closes", () => {
    vi.useFakeTimers();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    mountChat();
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledOnce();

    for (let i = 0; i < 6; i += 1) {
      act(() => closeSocket?.());
      act(() => {
        vi.advanceTimersByTime(0);
      });
    }

    expect(webSessionTransport.connectWebSessionTransport.mock.calls.length).toBeGreaterThan(6);
    expect(screen.queryByText("Lost the session connection. Reopen the task to try again.")).not
      .toBeInTheDocument();
    vi.useRealTimers();
  });

  it("never records a message as sent while the socket is down", () => {
    mountChat();
    act(() => closeSocket?.());
    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "ship it" } });
    fireEvent.submit(screen.getByRole("form", { name: "Session composer" }));
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(screen.queryByTestId("session-message-user")).not.toBeInTheDocument();
    // Draft stays so the operator can hit Enter once the socket returns.
    expect(screen.getByLabelText("Message")).toHaveValue("ship it");
    expect(screen.queryByRole("button", { name: "Send" })).not.toBeInTheDocument();
  });

  it("never clears a pending decision while the socket is down", () => {
    mountChat();
    send({ type: "permission_request", requestId: "42", title: "Run cargo test?" });
    act(() => closeSocket?.());
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(transport.respondPermission).not.toHaveBeenCalled();
    // The agent is still blocked, so the decision must stay on screen.
    expect(screen.getByTestId("session-decision")).toBeInTheDocument();
  });

  it("shows one status vocabulary: the live head state, not a second lifecycle pill", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" } as BrowserTaskDetail,
    });
    // Idle ACP + running task: head says Ready (no duplicate lifecycle pill).
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "idle");
    expect(screen.getByText("Ready")).toBeInTheDocument();
    expect(screen.queryByText("Running")).not.toBeInTheDocument();

    send({ type: "message", role: "agent", text: "working" });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "working");
    expect(screen.getByText("Working")).toBeInTheDocument();
  });

  it("offers the task's actions in the head when the task needs a human", () => {
    mountChat();
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "attention");
    expect(screen.getByTestId("session-head-actions")).toBeInTheDocument();
  });

  it("keeps destructive actions out of the head's fast-tap row", () => {
    mountChat();
    const head = screen.getByTestId("session-head-actions");
    const destructive = (taskDetail as BrowserTaskDetail).actions.filter((a) => a.destructive);
    expect(destructive.length).toBeGreaterThan(0);
    for (const action of destructive) {
      expect(within(head).queryByRole("button", { name: action.label })).not.toBeInTheDocument();
    }
    // Still reachable, just not one fast tap away.
    fireEvent.click(screen.getByTestId("session-details"));
    const panel = screen.getByTestId("session-task-panel");
    expect(within(panel).getByRole("button", { name: destructive[0].label })).toBeInTheDocument();
  });

  it("disables the decision while the socket is down", () => {
    mountChat();
    send({ type: "permission_request", requestId: "42", title: "Run cargo test?" });
    expect(screen.getByRole("button", { name: "Approve" })).toBeEnabled();
    act(() => closeSocket?.());
    expect(screen.getByRole("button", { name: "Approve" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject" })).toBeDisabled();
  });

  it("records a transport error in the transcript and ends the turn", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    send({ type: "error", message: "ACP process exited" });
    expect(screen.getByTestId("session-note-error")).toHaveTextContent(
      "The agent stopped. It will restart when you reconnect.",
    );
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
  });

  it("offers a jump-to-live control once the reader scrolls off the live edge", () => {
    mountChat();
    const thread = screen.getByTestId("session-thread");
    Object.defineProperty(thread, "scrollHeight", { value: 1000, configurable: true });
    Object.defineProperty(thread, "clientHeight", { value: 300, configurable: true });
    thread.scrollTop = 0;

    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
    fireEvent.scroll(thread);
    // Scrolling away with nothing new must not claim the reader is behind.
    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();

    send({ type: "message", role: "agent", text: "more output" });
    expect(screen.getByTestId("session-jump")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("session-jump"));
    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
  });

  it("counts only the steps that arrived since the reader left the live edge", () => {
    mountChat();
    send({
      type: "tool_call",
      callId: "seen",
      title: "Read",
      kind: "read",
      status: "completed",
    });

    const thread = screen.getByTestId("session-thread");
    Object.defineProperty(thread, "scrollHeight", { value: 1000, configurable: true });
    Object.defineProperty(thread, "clientHeight", { value: 300, configurable: true });
    thread.scrollTop = 0;
    fireEvent.scroll(thread);

    send({
      type: "tool_call",
      callId: "new",
      title: "Edit",
      kind: "edit",
      status: "in_progress",
    });
    // One step arrived while away, even though the session has two.
    expect(screen.getByTestId("session-jump")).toHaveTextContent("1 new step");
  });

  it("keeps the ACP socket across a starter-context identity change", () => {
    const { rerender } = render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
        starterContext={{ title: "Fix login", constraints: "", expectedOutcome: "" }}
      />,
    );
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledOnce();
    rerender(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
        starterContext={{ title: "Fix login", constraints: "", expectedOutcome: "" }}
      />,
    );
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledOnce();
    expect(transport.dispose).not.toHaveBeenCalled();
  });

  it("offers model switching in Task details when idle", async () => {
    mountChat();
    expect(screen.queryByTestId("session-model-select")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    const select = await screen.findByTestId("session-model-select");
    fireEvent.change(select, {
      target: { value: "composer-2.5" },
    });
    expect(transport.setModel).toHaveBeenCalledWith("composer-2.5");
  });

  it("disables model switching while a turn is in flight", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-model-select")).toBeDisabled();
  });

  it("keeps the transcript when ready arrives on a live socket", () => {
    mountChat();
    send({ type: "message", role: "user", text: "hello" });
    send({ type: "message", role: "agent", text: "hi there" });
    act(() => signalReady?.("composer-2.5"));
    expect(screen.getByTestId("session-message-user")).toHaveTextContent("hello");
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("hi there");
  });

  it("keeps the ACP socket when the terminal escape hatch opens", async () => {
    mountChat();
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(screen.getByTestId("session-terminal-toggle"));
    expect(await screen.findByTestId("session-terminal-sheet")).toBeInTheDocument();
    expect(transport.dispose).not.toHaveBeenCalled();
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalledOnce();
  });

  it("keeps the composer inside the thread as a full-width strip, with no Send button", () => {
    mountChat();
    const thread = screen.getByTestId("session-thread");
    const composer = screen.getByTestId("session-composer");
    expect(thread).toContainElement(composer);
    expect(screen.queryByRole("button", { name: "Send" })).not.toBeInTheDocument();
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"), "utf8");
    expect(css).toMatch(/\.session-composer\s*\{[^}]*align-self:\s*stretch/);
    expect(css).toMatch(/\.session-composer\s*\{[^}]*margin:\s*0\s+-12px/);
    expect(css).toMatch(/\.session-composer\s+textarea\s*\{[^}]*background:\s*transparent/);
  });

  it("locks the chat thread to vertical scroll only", () => {
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"), "utf8");
    expect(css).toMatch(/\.session-page\.session-chat\s*\{[^}]*overflow-x:\s*hidden/);
    expect(css).toMatch(/\.session-page\.session-chat\s*\{[^}]*padding:\s*0/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*overflow-x:\s*hidden/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*overflow-y:\s*auto/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*scrollbar-width:\s*none/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*-ms-overflow-style:\s*none/);
    expect(css).toMatch(/\.session-thread::-webkit-scrollbar\s*\{[^}]*display:\s*none/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*gap:\s*16px/);
    expect(css).toMatch(/\.session-thread\s*\{[^}]*padding:\s*8px\s+12px\s+0/);
    expect(css).not.toMatch(/\.session-head\s*\{[^}]*margin:\s*0\s+-12px/);
  });

  it("lets the terminal sheet fill past the desktop 58vh task-panel cap", () => {
    // Desktop task detail sets `.terminal-panel .terminal-interaction-wrap` to
    // min(58vh, 560px). The session sheet must beat that with a more specific
    // flex fill or operators only get a half-height escape hatch.
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"), "utf8");
    expect(css).toMatch(
      /\.session-terminal-sheet\s+\.terminal-panel\s+\.terminal-interaction-wrap\s*\{[^}]*height:\s*auto/,
    );
    expect(css).toMatch(/\.session-composer\s+textarea\s*\{[^}]*min-height:\s*28px/);
    expect(css).toMatch(
      /html\.keyboard-open\s+\.session-composer\s*\{[^}]*padding:\s*2px\s+12px\s+0/,
    );
    expect(css).toMatch(
      /\.session-composer\s+textarea\s*\{[^}]*font-size:\s*var\(--text-body-sm\)/,
    );
  });

  it("gives the transcript an 80% flex basis so chrome stays a thin strip", () => {
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"), "utf8");
    expect(css).toMatch(/\.session-thread\s*\{[^}]*flex:\s*1\s+1\s+80%/);
    // Chat folds back/title into the live head — no second sticky header row.
    const source = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "./SessionChat.tsx"), "utf8");
    expect(source).not.toMatch(/className="session-header"/);
    expect(source).toMatch(/onBack=\{onBack/);
  });

  it("locks session polish CSS contracts", () => {
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"), "utf8");
    expect(css).toMatch(/\.session-head-back\s*\{[^}]*min-height:\s*44px/);
    expect(css).toMatch(/\.session-head-details,\s*\n\.session-head-stop\s*\{[^}]*min-height:\s*44px/);
    expect(css).toMatch(/\.session-head \.session-title\s*\{[^}]*font-size:\s*var\(--text-body\)/);
    expect(css).toMatch(/\.session-note\s*\{[^}]*font-size:\s*var\(--text-micro\)/);
    expect(css).toMatch(/\.session-note\s*\{[^}]*letter-spacing:\s*var\(--tracking-label\)/);
    expect(css).toMatch(/\.session-reply\.is-live\s*\{[^}]*color:\s*var\(--ink-soft\)/);
    expect(css).toMatch(/\.session-decision\s*\{[^}]*background:\s*var\(--paper-tint\)/);
    expect(css).toMatch(/\.session-decision\s*\{[^}]*border-radius:\s*var\(--radius\)/);
    expect(css).toMatch(/\.session-composer:focus-within\s*\{[^}]*border-top-color:\s*var\(--accent\)/);
    expect(css).toMatch(/\.session-thread-empty\s*\{[^}]*font-size:\s*var\(--text-body\)/);
  });
});
