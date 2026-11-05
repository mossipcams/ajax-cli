import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";
import {
  chatH,
  flushRaf,
  mountChat,
  prepareChatSurface,
  send,
  transport,
  typeComposer,
} from "./ChatSurface.testHarness";

const tool = (callId: string) => ({
  type: "tool_call" as const,
  callId,
  title: "Bash",
  kind: "execute",
  status: "in_progress",
});

beforeEach(() => {
  prepareChatSurface();
  let nextId = 0;
  transport.sendPrompt.mockImplementation(() => `cmid-${nextId++}`);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("ChatSurface interruption", () => {
  it("releases Stop & send when cancellation and the next turn arrive in one render (#1578)", () => {
    mountChat();
    typeComposer("first");
    send(tool("t1"));
    typeComposer("next");
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });

    expect(transport.sendCancel).toHaveBeenCalledExactlyOnceWith(true);
    expect(screen.getByTestId("session-queued")).toHaveTextContent("Stopping…");

    act(() => {
      chatH.emit?.({ type: "turn_end", stopReason: "cancelled" });
      chatH.emit?.(tool("t2"));
    });
    flushRaf();

    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Queue" })).toBeInTheDocument();
    expect(screen.getByTestId("session-note-info")).toHaveTextContent("Stopped");
    expect(transport.sendPrompt).toHaveBeenCalledTimes(2);

    transport.sendPrompt.mockClear();
    typeComposer("during");

    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("during", [], undefined);
    expect(screen.getByTestId("session-queued")).not.toHaveTextContent("Stopping…");
  });

  it("releases a queued prompt when normal completion and the next turn arrive together", () => {
    mountChat();
    typeComposer("first");
    send(tool("t1"));
    typeComposer("next");

    act(() => {
      chatH.emit?.({ type: "turn_end", stopReason: "end_turn" });
      chatH.emit?.(tool("t2"));
    });
    flushRaf();

    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(screen.queryByText("Stopped")).not.toBeInTheDocument();
    transport.sendPrompt.mockClear();
    typeComposer("during");
    expect(transport.sendPrompt).toHaveBeenCalledExactlyOnceWith("during", [], undefined);
  });

  it("holds a restored queue through mount replay until the live turn ends", () => {
    localStorage.setItem(
      "ajax.web.session.composer.queue.web%2Ffix-login",
      JSON.stringify({ text: "restored", clientMessageId: "cmid-restored" }),
    );
    mountChat();
    act(() => {
      chatH.emit?.({ type: "turn_end", stopReason: "end_turn" });
      chatH.emit?.(tool("t1"));
    });
    flushRaf();

    expect(screen.getByTestId("session-queued")).toHaveTextContent("restored");
    expect(transport.sendPrompt).not.toHaveBeenCalled();

    act(() => {
      chatH.emit?.({ type: "turn_end", stopReason: "end_turn" });
      chatH.emit?.(tool("t2"));
    });
    flushRaf();

    expect(screen.queryByTestId("session-queued")).not.toBeInTheDocument();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });
});
