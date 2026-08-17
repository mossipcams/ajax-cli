import { renderHook, act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import { writeSessionModel } from "./sessionModel";
import type { SessionAction } from "./sessionThread";
import { useSessionTransport } from "./useSessionTransport";

describe("useSessionTransport", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
  });

  it("does not pass the localStorage model preference on reconnect (#910)", () => {
    vi.useFakeTimers();
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const models: (string | undefined)[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks, _platform, model) => {
        callbacks.push(nextCallbacks);
        models.push(model);
        nextCallbacks.onReady(model ?? "auto");
        return transport;
      },
    );

    const { unmount } = renderHook(() =>
      useSessionTransport({
        handle: "web/fix-login",
        dispatch: vi.fn(),
        detailRef: { current: null },
        transportRef: { current: undefined },
        connectedRef: { current: false },
        everOpenedRef: { current: false },
        onActivity: vi.fn(),
        setConnected: vi.fn(),
        setEverOpened: vi.fn(),
      }),
    );

    writeSessionModel("composer-2.5");
    act(() => callbacks[0]?.onClosed());
    act(() => vi.advanceTimersByTime(0));

    expect(models).toEqual([undefined, undefined]);
    unmount();
  });

  // Regression for issue #904: streamed ACP message chunks must be coalesced
  // into phrase-sized bursts before reaching the reducer, not dispatched one
  // per token. Unbuffered, four chunks produce four `event` dispatches.
  it("coalesces streamed message chunks before dispatching", () => {
    vi.useFakeTimers();
    const dispatched: SessionAction[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks, _platform, model) => {
        callbacks.push(nextCallbacks);
        nextCallbacks.onReady(model ?? "auto");
        return transport;
      },
    );

    const { unmount } = renderHook(() =>
      useSessionTransport({
        handle: "web/fix-login",
        dispatch: (action) => dispatched.push(action),
        detailRef: { current: null },
        transportRef: { current: undefined },
        connectedRef: { current: false },
        everOpenedRef: { current: false },
        onActivity: vi.fn(),
        setConnected: vi.fn(),
        setEverOpened: vi.fn(),
      }),
    );

    const onEvent = callbacks[0]?.onEvent;
    expect(onEvent).toBeTruthy();

    // Four token-sized chunks arrive back-to-back.
    act(() => {
      onEvent!({ type: "message", role: "agent", text: "The ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug is ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug is here", itemId: "i1" });
    });

    const messageDispatches = dispatched.filter(
      (a) => a.type === "event" && a.event.type === "message",
    );
    // Still buffered — no per-token dispatch has reached the reducer.
    expect(messageDispatches).toHaveLength(0);

    // turn_end flushes the remaining buffer first, then the turn_end event.
    act(() => onEvent!({ type: "turn_end", stopReason: "end_turn" }));

    const events = dispatched
      .filter((a) => a.type === "event")
      .map((a) => (a as Extract<SessionAction, { type: "event" }>).event);
    expect(events).toEqual([
      { type: "message", role: "agent", text: "The bug is here", itemId: "i1" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    unmount();
  });

  it("does not reset reducer on reconnect when snapshot reset is false", () => {
    vi.useFakeTimers();
    const dispatched: SessionAction[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks) => {
        callbacks.push(nextCallbacks);
        return transport;
      },
    );

    const { unmount } = renderHook(() =>
      useSessionTransport({
        handle: "web/fix-login",
        dispatch: (action) => dispatched.push(action),
        detailRef: { current: null },
        transportRef: { current: undefined },
        connectedRef: { current: false },
        everOpenedRef: { current: false },
        onActivity: vi.fn(),
        setConnected: vi.fn(),
        setEverOpened: vi.fn(),
      }),
    );

    const onEvent = callbacks[0]?.onEvent;
    act(() => {
      callbacks[0]?.onReady("auto");
      onEvent?.({ type: "ready", busy: false, reset: false });
      onEvent?.({ type: "message", role: "agent", text: "one", itemId: "i1" });
      onEvent?.({ type: "message", role: "agent", text: "two", itemId: "i2" });
      onEvent?.({ type: "turn_end", stopReason: "end_turn" });
    });

    act(() => callbacks[0]?.onClosed());
    act(() => vi.advanceTimersByTime(0));

    const reconnect = callbacks[1]?.onEvent;
    act(() => {
      reconnect?.({ type: "ready", busy: false, reset: false });
      reconnect?.({ type: "message", role: "agent", text: "three", itemId: "i3" });
      reconnect?.({ type: "turn_end", stopReason: "end_turn" });
    });

    const messages = dispatched
      .filter((a) => a.type === "event" && a.event.type === "message")
      .map((a) => (a as Extract<SessionAction, { type: "event" }>).event);
    expect(messages).toEqual([
      { type: "message", role: "agent", text: "one", itemId: "i1" },
      { type: "message", role: "agent", text: "two", itemId: "i2" },
      { type: "message", role: "agent", text: "three", itemId: "i3" },
    ]);
    expect(dispatched.some((a) => a.type === "reset")).toBe(false);
    unmount();
  });

  it("passes in-memory cursor on in-page reconnect but not on first attach", () => {
    vi.useFakeTimers();
    const resumeCursors: (number | undefined)[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks, _platform, _model, resumeCursor) => {
        callbacks.push(nextCallbacks);
        resumeCursors.push(resumeCursor);
        return transport;
      },
    );

    const { unmount } = renderHook(() =>
      useSessionTransport({
        handle: "web/fix-login",
        dispatch: vi.fn(),
        detailRef: { current: null },
        transportRef: { current: undefined },
        connectedRef: { current: false },
        everOpenedRef: { current: false },
        onActivity: vi.fn(),
        setConnected: vi.fn(),
        setEverOpened: vi.fn(),
      }),
    );

    expect(resumeCursors).toEqual([undefined]);
    act(() => {
      callbacks[0]?.onReady("auto");
      callbacks[0]?.onCursorAdvance?.(4);
      callbacks[0]?.onClosed();
    });
    act(() => vi.advanceTimersByTime(0));
    expect(resumeCursors).toEqual([undefined, 4]);
    unmount();
  });
});
