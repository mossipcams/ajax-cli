import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act, within, render } from "@testing-library/react";
import { StrictMode } from "react";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import * as autoGrowModule from "@/features/chat/composer/autoGrow";
import * as useChatSpeechModule from "@/features/chat/composer/speech/useChatSpeech";
import { claimSessionViewportOwnership } from "@/shared/lib/sessionViewport";
import { initViewport, isKeyboardOpen } from "@/shared/lib/viewport";
import {
  chatH,
  flushRaf,
  mountChat,
  prepareChatSurface,
  send,
  typeComposer,
  transport,
  ChatWithSheet,
} from "../ChatSurface.testHarness";

const composerCssPath = join(
  dirname(fileURLToPath(import.meta.url)),
  "../../../styles/chat/composer.css",
);
const composerCss = readFileSync(composerCssPath, "utf8");

function stripCssComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
  document.documentElement.removeAttribute("data-session-viewport");
});

describe("ChatComposer", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  it("sends composer messages through ACP on Enter", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test", [], undefined);
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
    send({ type: "message", role: "user", text: "Please fix the flaky test", itemId: "u1" });
    expect(screen.getAllByTestId("session-message-user")).toHaveLength(1);
  });

  it("queues one editable follow-up and stages it on the host while a turn is live", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", [], undefined);
    expect(transport.sendCancel).not.toHaveBeenCalled();
    const queued = screen.getByTestId("session-queued");
    expect(queued).toHaveTextContent("Queued");
    expect(queued).toHaveTextContent("Next");
    expect(queued).toHaveTextContent("Press Enter again to stop and send now");
    expect(screen.getByRole("button", { name: "Stop & send" })).toBeInTheDocument();
  });

  it("does not resend a staged follow-up when the turn ends normally", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", [], undefined);
    send({ type: "message", role: "user", text: "Next", itemId: "u:cmid-1" });
    send({ type: "turn_end", stopReason: "end_turn" });

    expect(transport.sendPrompt).toHaveBeenCalledTimes(1);
    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("session-message-user").at(-1)).toHaveTextContent("Next");
  });

  it("stages attachment-bearing follow-ups on the host while busy", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });

    typeComposer("First");
    transport.sendPrompt.mockClear();

    const file = new File(["hello"], "photo.jpg", { type: "image/jpeg" });
    const input = screen.getByLabelText("Message");
    fireEvent.paste(input, {
      clipboardData: {
        items: [{ kind: "file", type: "image/jpeg", getAsFile: () => file }],
      },
    });
    expect(await screen.findByText("photo.jpg")).toBeInTheDocument();

    typeComposer("Next");
    send({ type: "turn_end", stopReason: "end_turn" });

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "Next",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
      undefined,
    );
  });

  it("stops the turn on a second Enter without resending the staged follow-up", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });

    expect(transport.sendCancel).toHaveBeenCalledTimes(1);
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", [], undefined);
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Stopping…");

    send({ type: "turn_end", stopReason: "cancelled" });

    expect(transport.sendPrompt).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("session-note-info")).toHaveTextContent("Stopped");
  });

  it("calls sendCancel exactly once on empty submit while queued under StrictMode", () => {
    render(
      <StrictMode>
        <ChatWithSheet />
      </StrictMode>,
    );

    typeComposer("First");
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    typeComposer("Next");
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });

    expect(transport.sendCancel).toHaveBeenCalledTimes(1);
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", [], undefined);
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Stopping…");
  });

  // ajax-cli#1081: typing a new message while a follow-up is queued replaces it without cancelling.
  it("replaces a queued follow-up when the operator types again (#1081)", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    typeComposer("A");
    expect(screen.getByTestId("session-queued")).toHaveTextContent("A");
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("A", [], undefined);
    expect(transport.sendCancel).not.toHaveBeenCalled();

    transport.sendPrompt.mockClear();
    typeComposer("B");

    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("B", [], "cmid-1");
    expect(screen.getByTestId("session-queued")).toHaveTextContent("B");
    expect(screen.getByLabelText("Message")).toHaveValue("");
  });

  it("withdraws the host row when the operator edits a staged follow-up", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(transport.withdrawQueuedPrompt).toHaveBeenCalledWith("cmid-1");
    expect(screen.getByLabelText("Message")).toHaveValue("Next");
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
  });

  it("withdraws the host row when the operator removes a staged follow-up", () => {
    mountChat();

    typeComposer("First");
    transport.sendPrompt.mockClear();
    typeComposer("Next");
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(transport.withdrawQueuedPrompt).toHaveBeenCalledWith("cmid-1");
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();

    send({ type: "turn_end", stopReason: "end_turn" });
    expect(transport.sendPrompt).toHaveBeenCalledTimes(1);
    expect(transport.sendPrompt).toHaveBeenCalledWith("Next", [], undefined);
  });

  it("names what Enter will do next on the composer action", () => {
    mountChat();

    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
    typeComposer("First");
    expect(screen.getByRole("button", { name: "Queue" })).toBeInTheDocument();
    typeComposer("Next");
    expect(screen.getByRole("button", { name: "Stop & send" })).toBeInTheDocument();
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

  it("passes slash commands through session/prompt unchanged", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "/web query" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("/web query", [], undefined);
  });

  it("sends /clear through the clear wire command instead of session/prompt", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "/clear" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendClear).toHaveBeenCalledOnce();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(screen.queryByTestId("session-message-user")).not.toBeInTheDocument();
  });

  it("offers /clear in slash completion without advertised commands", () => {
    mountChat();
    const input = screen.getByLabelText("Message");
    fireEvent.change(input, { target: { value: "/cl" } });
    expect(screen.getByTestId("session-composer-slash-menu")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/clear/ })).toBeInTheDocument();
  });

  it("shows advertised slash matches and inserts on Tab without submitting", () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        availableCommands: [
          { name: "web", description: "Query the web", inputHint: "query" },
          { name: "help", description: "Show help" },
        ],
      });
    });
    const input = screen.getByLabelText("Message");
    fireEvent.change(input, { target: { value: "/w" } });
    expect(screen.getByTestId("session-composer-slash-menu")).toBeInTheDocument();
    transport.sendPrompt.mockClear();
    fireEvent.keyDown(input, { key: "Tab" });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(input).toHaveValue("/web ");
  });

  it("lets operators tap a slash command row on touch devices", () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        availableCommands: [{ name: "help", description: "Show help" }],
      });
    });
    const input = screen.getByLabelText("Message");
    fireEvent.change(input, { target: { value: "/" } });
    transport.sendPrompt.mockClear();
    fireEvent.click(screen.getByRole("option", { name: /\/help/ }));
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(input).toHaveValue("/help");
  });

  it("places attach, model, mic, and send on the hotbar with message textarea below", () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    const composer = screen.getByTestId("session-composer");
    const hotbar = screen.getByTestId("session-composer-hotbar");
    const textarea = screen.getByLabelText("Message");

    expect(hotbar).toBeInTheDocument();
    expect(composer).toContainElement(hotbar);
    expect(composer).toContainElement(textarea);
    expect(hotbar).not.toContainElement(textarea);

    const hotbarScope = within(hotbar);
    expect(hotbarScope.getByRole("button", { name: "Attach" })).toBeInTheDocument();
    expect(hotbarScope.getByRole("button", { name: /voice input/i })).toBeInTheDocument();
    expect(hotbarScope.getByRole("button", { name: "Send" })).toBeInTheDocument();

    expect(hotbar.compareDocumentPosition(textarea) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("uses icon-only attach and send controls with accessible names", () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    const attach = screen.getByRole("button", { name: "Attach" });
    const sendButton = screen.getByRole("button", { name: "Send" });

    expect(attach).toHaveTextContent("");
    expect(sendButton).toHaveTextContent("");
  });

  it("clears stale keyboard band geometry after composer blur when visualViewport stays shrunken", () => {
    const vvListeners: Record<string, Array<() => void>> = {};
    let vvHeight = 800;
    vi.stubGlobal("innerHeight", 800);
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      cb(0);
      return 1;
    });
    vi.stubGlobal("visualViewport", {
      get height() {
        return vvHeight;
      },
      get offsetTop() {
        return 0;
      },
      addEventListener: (type: string, handler: () => void) => {
        (vvListeners[type] ??= []).push(handler);
      },
      removeEventListener: vi.fn(),
    });
    window.scrollTo = vi.fn();

    claimSessionViewportOwnership();
    const dispose = initViewport();
    mountChat();

    const textarea = screen.getByLabelText("Message");
    act(() => textarea.focus());
    vvHeight = 500;
    for (const handler of vvListeners.resize ?? []) handler();
    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("500px");

    act(() => {
      textarea.blur();
      document.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
    expect(
      screen.getByTestId("session-chat-surface").style.paddingBottom,
    ).toBe("");

    dispose();
  });

  it("hugs the bottom edge with no safe-area pad on composer or textarea (#1034)", () => {
    const composerRule =
      composerCss.match(/\.session-composer\s*\{([^}]*)\}/)?.[1] ?? "";
    const textareaRule =
      composerCss.match(/\.session-composer textarea\s*\{([^}]*)\}/)?.[1] ?? "";
    const keyboardTextareaRule =
      composerCss.match(/html\.keyboard-open\s+\.session-composer textarea\s*\{([^}]*)\}/)?.[1] ??
      "";
    const attachRule =
      composerCss.match(/\.session-composer-actions\s*\{([^}]*)\}/)?.[1] ?? "";
    const sendRule =
      composerCss.match(/\.session-composer-send\s*\{([^}]*)\}/)?.[1] ?? "";

    const composerBody = stripCssComments(composerRule);
    const textareaBody = stripCssComments(textareaRule);
    const keyboardTextareaBody = stripCssComments(keyboardTextareaRule);
    const actionsBody = stripCssComments(attachRule);
    const sendBody = stripCssComments(sendRule);

    expect(composerBody).not.toMatch(/env\(safe-area-inset-bottom\)/);
    expect(textareaBody).not.toMatch(/env\(safe-area-inset-bottom\)/);
    expect(keyboardTextareaBody).not.toMatch(/env\(safe-area-inset-bottom\)/);
    expect(actionsBody).toMatch(/margin-left:\s*auto/);
    expect(sendBody).not.toMatch(/margin-left:\s*auto/);
  });

  it("restores unsent composer text after leaving and returning to the task", () => {
    const { unmount } = mountChat({ handle: "web/fix-login" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "draft before navigate" },
    });
    unmount();
    mountChat({ handle: "web/fix-login" });
    expect(screen.getByLabelText("Message")).toHaveValue("draft before navigate");
  });

  it("auto-grows the textarea when restoring a stored multiline draft", () => {
    const autoGrowSpy = vi.spyOn(autoGrowModule, "autoGrow");
    const multiline = "line one\nline two\nline three";

    const { unmount } = mountChat({ handle: "web/fix-login" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: multiline },
    });
    unmount();

    autoGrowSpy.mockClear();
    mountChat({ handle: "web/fix-login" });

    expect(screen.getByLabelText("Message")).toHaveValue(multiline);
    expect(autoGrowSpy).toHaveBeenCalledWith(expect.any(HTMLTextAreaElement), true);
  });

  it("isolates composer drafts per task handle", () => {
    let view = mountChat({ handle: "web/task-a" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "alpha draft" },
    });
    view.unmount();
    view = mountChat({ handle: "web/task-b" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "beta draft" },
    });
    view.unmount();
    view = mountChat({ handle: "web/task-a" });
    expect(screen.getByLabelText("Message")).toHaveValue("alpha draft");
    view.unmount();
    mountChat({ handle: "web/task-b" });
    expect(screen.getByLabelText("Message")).toHaveValue("beta draft");
  });

  it("clears stored draft after a successful send", () => {
    const { unmount } = mountChat({ handle: "web/fix-login" });
    typeComposer("sent and cleared");
    unmount();
    mountChat({ handle: "web/fix-login" });
    expect(screen.getByLabelText("Message")).toHaveValue("");
  });

  it("persists unsent drafts in localStorage so they survive tab close", () => {
    const { unmount } = mountChat({ handle: "web/fix-login" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "survives tab close" },
    });
    unmount();
    expect(localStorage.getItem("ajax.web.session.composer.draft.web%2Ffix-login")).toBe(
      "survives tab close",
    );
    expect(sessionStorage.getItem("ajax.web.session.composer.draft.web%2Ffix-login")).toBeNull();
    mountChat({ handle: "web/fix-login" });
    expect(screen.getByLabelText("Message")).toHaveValue("survives tab close");
  });

  it("restores a queued follow-up after leaving and returning to the task", () => {
    const { unmount } = mountChat({ handle: "web/fix-login" });
    typeComposer("First");
    typeComposer("Next");
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Next");
    unmount();
    mountChat({ handle: "web/fix-login" });
    act(() => {
      chatH.emit?.({ type: "ready", model: "auto", busy: true, reset: false });
    });
    flushRaf();
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Next");
    expect(localStorage.getItem("ajax.web.session.composer.queue.web%2Ffix-login")).toContain(
      "Next",
    );
  });

  it("restores the failed prompt into the composer when a turn ends without an agent answer", () => {
    mountChat();
    typeComposer("Please fix the flaky test");
    expect(screen.getByLabelText("Message")).toHaveValue("");
    transport.sendPrompt.mockClear();

    send({ type: "turn_end", stopReason: "error" });

    expect(screen.getByLabelText("Message")).toHaveValue("Please fix the flaky test");
    expect(screen.getByTestId("session-note-error")).toBeInTheDocument();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });

  it("does not overwrite a non-empty composer draft when a turn fails", () => {
    localStorage.setItem(
      "ajax.web.session.composer.draft.web%2Ffix-login",
      "operator replacement",
    );
    mountChat();
    expect(screen.getByLabelText("Message")).toHaveValue("operator replacement");
    act(() => {
      chatH.emit?.({ type: "ready", model: "auto", busy: true, reset: false });
    });
    send({ type: "message", role: "user", text: "sent prompt", itemId: "u1" });
    transport.sendPrompt.mockClear();

    send({ type: "turn_end", stopReason: "error" });

    expect(screen.getByLabelText("Message")).toHaveValue("operator replacement");
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });

  it("does not restore the prompt when the agent already answered before the error", () => {
    mountChat();
    typeComposer("go");
    send({
      type: "message",
      role: "agent",
      text: "Partial answer before disconnect",
      itemId: "a1",
    });
    transport.sendPrompt.mockClear();

    send({ type: "turn_end", stopReason: "error" });

    expect(screen.getByLabelText("Message")).toHaveValue("");
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });

  it("restores a failed prompt only once even if the operator clears the composer", () => {
    mountChat({ handle: "web/fix-login" });
    typeComposer("retry me");
    send({ type: "turn_end", stopReason: "error" });
    expect(screen.getByLabelText("Message")).toHaveValue("retry me");

    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "" } });
    flushRaf();

    expect(screen.getByLabelText("Message")).toHaveValue("");
    expect(localStorage.getItem("ajax.web.session.composer.draft.web%2Ffix-login")).toBeNull();
  });

  it("persists a restored failed prompt in localStorage", () => {
    mountChat({ handle: "web/fix-login" });
    typeComposer("survives reload");
    send({ type: "turn_end", stopReason: "error" });
    expect(localStorage.getItem("ajax.web.session.composer.draft.web%2Ffix-login")).toBe(
      "survives reload",
    );
  });
});
