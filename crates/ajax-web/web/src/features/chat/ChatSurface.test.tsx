import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act } from "@testing-library/react";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import * as webSessionTransport from "./session/transport/public";
import {
  ChatWithSheet,
  chatH,
  mountChat,
  prepareChatSurface,
  send,
  transport,
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

  it("composes the live head, transcript, and composer", () => {
    mountChat();
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
  });

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
        callbacks.onEvent({ type: "turn_end", stopReason: "end_turn" });
        callbacks.onReady("auto");
        return transport;
      },
    );

    mountChat();

    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Earlier reply");
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

  it("opens Diff Review when swiping on transcript text without active highlighting", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    mountChat({ onOpenDiff });
    send({ type: "message", role: "agent", text: "Selectable answer", itemId: "a1" });
    send({ type: "turn_end", stopReason: "end_turn" });

    const root = screen.getByTestId("session-chat");
    const message = screen.getByTestId("session-message-agent");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });

    fireEvent.touchStart(message, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    fireEvent.touchMove(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("does not open Diff Review when transcript highlighting activates after touchstart (#1051)", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    mountChat({ onOpenDiff });
    send({ type: "message", role: "agent", text: "Selectable answer", itemId: "a1" });
    send({ type: "turn_end", stopReason: "end_turn" });

    const root = screen.getByTestId("session-chat");
    const message = screen.getByTestId("session-message-agent");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });

    fireEvent.touchStart(message, { changedTouches: [{ clientX: 200, clientY: 40 }] });

    const range = document.createRange();
    range.selectNodeContents(message);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    fireEvent.touchMove(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).not.toHaveBeenCalled();
    selection?.removeAllRanges();
    vi.useRealTimers();
  });

  it("does not open Diff Review when swiping on transcript text with active highlighting", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    mountChat({ onOpenDiff });
    send({ type: "message", role: "agent", text: "Selectable answer", itemId: "a1" });
    send({ type: "turn_end", stopReason: "end_turn" });

    const root = screen.getByTestId("session-chat");
    const message = screen.getByTestId("session-message-agent");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });

    const range = document.createRange();
    range.selectNodeContents(message);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    fireEvent.touchStart(message, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    fireEvent.touchMove(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(message, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).not.toHaveBeenCalled();
    selection?.removeAllRanges();
    vi.useRealTimers();
  });

  it("closes the task details sheet when Drop confirm arms (#947)", () => {
    const { rerender } = mountChat({ pendingConfirmAction: null });
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();

    rerender(<ChatWithSheet pendingConfirmAction="drop" />);
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
  });
});
