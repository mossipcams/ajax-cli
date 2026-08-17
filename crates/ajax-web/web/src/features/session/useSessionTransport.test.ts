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
      onEvent!({ type: "message", role: "agent", text: "The " });
      onEvent!({ type: "message", role: "agent", text: "The bug " });
      onEvent!({ type: "message", role: "agent", text: "The bug is " });
      onEvent!({ type: "message", role: "agent", text: "The bug is here" });
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
      { type: "message", role: "agent", text: "The bug is here" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    unmount();
  });
});
