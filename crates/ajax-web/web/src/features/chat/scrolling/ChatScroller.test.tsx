import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act } from "@testing-library/react";
import { mountChat, prepareChatSurface, send, transport, flushRaf, ChatWithSheet, chatH } from "../ChatSurface.testHarness";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("ChatScroller integration", () => {
  beforeEach(() => {
    prepareChatSurface();
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

  it("does not blur the composer when tapping hotbar dead space", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    const hotbar = screen.getByTestId("session-composer-hotbar");
    const gap = document.createElement("span");
    gap.className = "hotbar-gap-probe";
    hotbar.appendChild(gap);

    const event = new TouchEvent("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: gap, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");
    hotbar.dispatchEvent(event);

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(composer).toHaveFocus();
  });

  it("does not blur the composer when tapping a hotbar action", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    const sendButton = screen.getByRole("button", { name: "Send" });
    const event = new TouchEvent("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: sendButton, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");
    sendButton.dispatchEvent(event);

    expect(preventDefault).not.toHaveBeenCalled();
    fireEvent.pointerDown(sendButton);
    expect(composer).toHaveFocus();
  });

  it("still submits from Send after hotbar focus retention", () => {
    mountChat();
    const composer = screen.getByLabelText("Message");
    composer.focus();
    fireEvent.change(composer, { target: { value: "hello" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(transport.sendPrompt).toHaveBeenCalledOnce();
  });

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
      for (const callback of resizeCallbacks) {
        callback([{ target: thread } as ResizeObserverEntry], {} as ResizeObserver);
      }
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
});

describe("Jump to latest", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  function scrollAway() {
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 2000 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
    thread.scrollTop = 120;
    fireEvent.scroll(thread);
    return thread;
  }

  // The button waited for new content to arrive while scrolled up, so a settled
  // transcript offered no way back to the bottom but a long drag on a phone.
  it("offers the way back whenever the reader is away from the live edge", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "Only message", itemId: "a1" });
    send({ type: "turn_end", stopReason: "end_turn" });
    scrollAway();

    expect(screen.getByTestId("session-jump")).toBeInTheDocument();
  });

  it("takes the reader back to the live edge and stands down", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "Only message", itemId: "a1" });
    const thread = scrollAway();

    fireEvent.click(screen.getByTestId("session-jump"));

    expect(thread.scrollTop).toBe(thread.scrollHeight);
    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
  });

  it("stays out of the way at the live edge", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "Only message", itemId: "a1" });

    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
  });
});

describe("task open positioning (#1065)", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  it("lands on the live edge after cached transcript hydrates (#1065)", () => {
    chatH.autoReady = false;
    mountChat();
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });

    for (let i = 0; i < 6; i += 1) {
      send({ type: "message", role: "user", text: `Question ${i}`, itemId: `u${i}` });
      send({
        type: "message",
        role: "agent",
        text: `Answer ${i}. ${"Padding ".repeat(40)}`,
        itemId: `a${i}`,
      });
      Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 3200 });
    }

    send({ type: "turn_end", stopReason: "end_turn" });
    act(() => chatH.ready?.("auto"));
    flushRaf();

    expect(thread.scrollTop).toBe(thread.scrollHeight);
    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
  });

  it("re-pins to the live edge when session identity changes on task open", () => {
    const { rerender } = mountChat({ detailStatus: "loading" });
    const thread = screen.getByTestId("session-thread") as HTMLDivElement;
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 2400 });

    for (let i = 0; i < 4; i += 1) {
      send({ type: "message", role: "agent", text: `Earlier ${i}`, itemId: `a${i}` });
    }
    thread.scrollTop = 80;
    fireEvent.scroll(thread);
    expect(screen.getByTestId("session-jump")).toBeInTheDocument();

    rerender(<ChatWithSheet detailStatus="ready" />);
    flushRaf();

    expect(thread.scrollTop).toBe(thread.scrollHeight);
    expect(screen.queryByTestId("session-jump")).not.toBeInTheDocument();
  });
});
