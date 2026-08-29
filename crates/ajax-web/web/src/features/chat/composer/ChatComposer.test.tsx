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

function mockImageCompressionCanvas(options?: { deferImageLoad?: boolean }) {
  const pendingImages: HTMLImageElement[] = [];
  const nativeCreateElement = Document.prototype.createElement;
  vi.spyOn(document, "createElement").mockImplementation(function (this: Document, tagName: string) {
    const element = nativeCreateElement.call(this, tagName);
    if (tagName !== "canvas") return element;
    const canvas = element as HTMLCanvasElement;
    canvas.getContext = vi.fn(() => ({
      drawImage: vi.fn(),
    })) as unknown as HTMLCanvasElement["getContext"];
    canvas.toDataURL = vi.fn((_type?: string, quality?: number) => {
      const scale = typeof quality === "number" ? quality : 0.92;
      const targetLen = Math.max(512, Math.floor(1200 * scale));
      return `data:image/jpeg;base64,${"B".repeat(targetLen)}`;
    });
    return canvas;
  });
  vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
    Object.defineProperty(this, "naturalWidth", { value: 4000 });
    Object.defineProperty(this, "naturalHeight", { value: 3000 });
    if (options?.deferImageLoad) {
      let assignedOnload: ((ev: Event) => void) | null = null;
      Object.defineProperty(this, "onload", {
        get: () => assignedOnload,
        set: (fn) => {
          assignedOnload = fn;
        },
        configurable: true,
      });
      Object.defineProperty(this, "src", {
        set: () => {
          pendingImages.push(this);
        },
        get: () => "",
        configurable: true,
      });
      return this;
    }
    setTimeout(() => this.onload?.(new Event("load")), 0);
    return this;
  } as unknown as typeof Image);
  return {
    async flushPendingImageLoads() {
      await vi.waitFor(() => {
        expect(pendingImages.length).toBeGreaterThan(0);
      });
      for (const img of pendingImages.splice(0)) {
        img.onload?.(new Event("load"));
      }
    },
  };
}

async function waitForAttachmentReady() {
  const sendButton = screen.getByRole("button", { name: /Send|Queue|Stop & send/ });
  await vi.waitFor(() => expect(sendButton).toBeEnabled());
}

async function pasteImageFile(file: File) {
  fireEvent.paste(screen.getByLabelText("Message"), {
    clipboardData: {
      items: [{ kind: "file", type: file.type, getAsFile: () => file }],
    },
  });
  expect(await screen.findByText(new RegExp(file.name))).toBeInTheDocument();
  await waitForAttachmentReady();
}

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
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test", []);
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
    send({ type: "message", role: "user", text: "Please fix the flaky test", itemId: "u1" });
    expect(screen.getAllByTestId("session-message-user")).toHaveLength(1);
  });

  // ajax-cli#1110: attached photos are complete prompts without caption text.
  it("enables Send and dispatches an attachment-only photo", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });

    const file = new File(["hello"], "photo.jpg", { type: "image/jpeg" });
    await pasteImageFile(file);
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
    );
  });

  it("does not send caption-only while a pasted photo is still preparing", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    const { flushPendingImageLoads } = mockImageCompressionCanvas({ deferImageLoad: true });

    const hugeData = "A".repeat(300_000);
    const file = new File([hugeData], "large.jpg", { type: "image/jpeg" });
    fireEvent.paste(screen.getByLabelText("Message"), {
      clipboardData: {
        items: [{ kind: "file", type: "image/jpeg", getAsFile: () => file }],
      },
    });
    expect(await screen.findByText(/large\.jpg.*Preparing/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "What is this?" },
    });
    transport.sendPrompt.mockClear();
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).not.toHaveBeenCalled();

    await act(async () => {
      await flushPendingImageLoads();
    });
    await waitForAttachmentReady();

    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "What is this?",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
    );
  });

  // ajax-cli#1110: large photos compress on attach; Send uses the sync fit path with caption.
  it("prepares a large pasted photo on attach and sends it with a caption", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    mockImageCompressionCanvas();

    const hugeData = "A".repeat(300_000);
    const file = new File([hugeData], "large.jpg", { type: "image/jpeg" });
    await pasteImageFile(file);

    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "What is this?" },
    });
    transport.sendPrompt.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "What is this?",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
    );
  });

  it("shows a chip error when photo preparation fails instead of silently ignoring Send", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      setTimeout(() => this.onerror?.(new Event("error")), 0);
      return this;
    } as unknown as typeof Image);

    const file = new File(["x".repeat(300_000)], "bad.jpg", { type: "image/jpeg" });
    fireEvent.paste(screen.getByLabelText("Message"), {
      clipboardData: {
        items: [{ kind: "file", type: "image/jpeg", getAsFile: () => file }],
      },
    });
    expect(await screen.findByText(/bad\.jpg.*too large/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();

    transport.sendPrompt.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });

  it("queues an attachment-only photo while a turn is busy", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });
    typeComposer("First");
    transport.sendPrompt.mockClear();

    const file = new File(["hello"], "photo.jpg", { type: "image/jpeg" });
    await pasteImageFile(file);
    fireEvent.click(screen.getByRole("button", { name: "Queue" }));

    expect(screen.getByTestId("session-queued")).toHaveTextContent("Queued");
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
    );
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

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", []);
    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("session-message-user").at(-1)).toHaveTextContent("Next");
  });

  it("sends the queued follow-up with attachments when the turn ends normally", async () => {
    mountChat();
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
        promptCapabilities: { image: true, embeddedContext: false },
      });
    });

    typeComposer("First");
    transport.sendPrompt.mockClear();

    const file = new File(["hello"], "photo.jpg", { type: "image/jpeg" });
    await pasteImageFile(file);

    typeComposer("Next");
    send({ type: "turn_end", stopReason: "end_turn" });

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith(
      "Next",
      expect.arrayContaining([expect.objectContaining({ type: "image", mimeType: "image/jpeg" })]),
    );
  });

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

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("Next", []);
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
    expect(transport.sendPrompt).not.toHaveBeenCalled();
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
    expect(transport.sendCancel).not.toHaveBeenCalled();

    typeComposer("B");

    expect(transport.sendCancel).not.toHaveBeenCalled();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(screen.getByTestId("session-queued")).toHaveTextContent("B");
    expect(screen.getByLabelText("Message")).toHaveValue("");
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
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("First", []);
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
    expect(transport.sendPrompt).toHaveBeenCalledWith("/web query", []);
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
        contextState: "live",
        contextEpoch: 0,
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
        contextState: "live",
        contextEpoch: 0,
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
        contextState: "live",
        contextEpoch: 0,
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
        contextState: "live",
        contextEpoch: 0,
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

  function emitContextSnapshot(
    contextState: "live" | "restored" | "unavailable",
    contextEpoch = 0,
    contextError?: string,
    transcriptError?: string,
  ) {
    act(() => {
      chatH.snapshot?.({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState,
        contextEpoch,
        ...(contextError !== undefined ? { contextError } : {}),
        ...(transcriptError !== undefined ? { transcriptError } : {}),
      });
    });
  }

  it("disables Send and shows Retry plus confirmed Start new when context is unavailable", () => {
    mountChat();
    emitContextSnapshot("unavailable", 2, "resume timed out");

    expect(screen.getByTestId("session-context-notice")).toHaveTextContent("resume timed out");
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Should not send" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Retry restore" }));
    expect(transport.retryRestore).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Start new context" }));
    expect(screen.getByTestId("result-panel-confirm")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(transport.startNewContext).toHaveBeenCalledOnce();
  });

  it.each(["live", "restored"] as const)(
    "leaves the normal composer unchanged when context is %s",
    (contextState) => {
      mountChat();
      emitContextSnapshot(contextState, contextState === "restored" ? 3 : 0);

      expect(screen.queryByTestId("session-context-notice")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Retry restore" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Start new context" })).not.toBeInTheDocument();

      fireEvent.change(screen.getByLabelText("Message"), {
        target: { value: "Still works" },
      });
      expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
    },
  );

  it("disables Send and shows a durability notice when transcriptError is set", () => {
    mountChat();
    emitContextSnapshot("live", 1, undefined, "forced append failure");

    expect(screen.getByTestId("session-transcript-notice")).toHaveTextContent(
      "forced append failure",
    );
    expect(screen.queryByTestId("session-context-notice")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Should not send" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });
});
