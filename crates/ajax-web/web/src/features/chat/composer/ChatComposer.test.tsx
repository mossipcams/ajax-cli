import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act } from "@testing-library/react";
import * as useChatSpeechModule from "@/features/chat/composer/speech/useChatSpeech";
import {
  chatH,
  mountChat,
  prepareChatSurface,
  send,
  typeComposer,
  transport,
} from "../ChatSurface.testHarness";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
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
});
