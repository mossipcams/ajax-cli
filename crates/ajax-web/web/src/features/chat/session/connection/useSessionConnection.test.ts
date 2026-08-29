import { renderHook, act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "../transport/public";
import { writeSessionModel } from "@/features/task/public";
import type { ChatSessionAction } from "../public";
import { initialConnectionState } from "./connectionState";
import { useSessionConnection } from "./useSessionConnection";

function hookOptions(overrides: Partial<Parameters<typeof useSessionConnection>[0]> = {}) {
  return {
    handle: "web/fix-login",
    dispatch: vi.fn(),
    detailRef: { current: null },
    transportRef: { current: undefined },
    connectionStateRef: { current: initialConnectionState() },
    everOpenedRef: { current: false },
    onActivity: vi.fn(),
    setConnectionState: vi.fn(),
    setEverOpened: vi.fn(),
    ...overrides,
  };
}

describe("useSessionConnection", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
  });

  it("does not pass the localStorage model preference on reconnect (#910)", () => {
    vi.useFakeTimers();
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const models: (string | undefined)[] = [];
    const hostModels: string[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      setConfigOption: vi.fn(),
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
      useSessionConnection({
        ...hookOptions(),
        onSessionModel: (model) => hostModels.push(model),
      }),
    );

    writeSessionModel("composer-2.5");
    act(() => callbacks[0]?.onClosed());
    act(() => vi.advanceTimersByTime(0));

    expect(models).toEqual([undefined, undefined]);
    expect(hostModels).toEqual(["auto", "auto"]);
    unmount();
  });

  // Regression for issue #931: host snapshots seed the New Task preference
  // through onSessionModel, not by reading localStorage as live session truth.
  it("reports host snapshot models through onSessionModel (#931)", () => {
    const hostModels: string[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      setConfigOption: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks) => {
        nextCallbacks.onReady("gpt-5.6-sol-medium");
        return transport;
      },
    );

    const { unmount } = renderHook(() =>
      useSessionConnection({
        ...hookOptions(),
        onSessionModel: (model) => hostModels.push(model),
      }),
    );

    writeSessionModel("composer-2.5");
    expect(hostModels).toEqual(["gpt-5.6-sol-medium"]);
    expect(localStorage.getItem("ajax.web.session.model")).toBe("composer-2.5");
    unmount();
  });

  // Regression for issue #904: streamed ACP message chunks must be coalesced
  // with requestAnimationFrame before reaching the reducer, not dispatched one
  // per token or held until turn_end.
  it("coalesces streamed message chunks before dispatching", () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const frameQueue: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frameQueue.push(callback);
      return frameQueue.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});

    const dispatched: ChatSessionAction[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      setConfigOption: vi.fn(),
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
      useSessionConnection({
        ...hookOptions(),
        dispatch: (action) => dispatched.push(action),
      }),
    );

    const onEvent = callbacks[0]?.onEvent;
    expect(onEvent).toBeTruthy();

    act(() => {
      onEvent!({ type: "message", role: "agent", text: "The ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug is ", itemId: "i1" });
      onEvent!({ type: "message", role: "agent", text: "The bug is here", itemId: "i1" });
    });

    const messageDispatches = dispatched.filter(
      (a) => a.type === "event" && a.event.type === "agent_message",
    );
    expect(messageDispatches).toHaveLength(0);

    act(() => {
      for (const callback of frameQueue.splice(0)) callback(0);
    });
    expect(
      dispatched.filter((a) => a.type === "event" && a.event.type === "agent_message"),
    ).toEqual([
      {
        type: "event",
        event: { type: "agent_message", text: "The bug is here", itemId: "i1" },
      },
    ]);

    act(() => onEvent!({ type: "turn_end", stopReason: "end_turn" }));

    const events = dispatched
      .filter((a) => a.type === "event")
      .map((a) => (a as Extract<ChatSessionAction, { type: "event" }>).event);
    expect(events).toEqual([
      { type: "agent_message", text: "The bug is here", itemId: "i1" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    unmount();
  });

  it("does not reset reducer on reconnect when snapshot reset is false", () => {
    vi.useFakeTimers();
    const dispatched: ChatSessionAction[] = [];
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      setConfigOption: vi.fn(),
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
      useSessionConnection({
        ...hookOptions(),
        dispatch: (action) => dispatched.push(action),
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
      .filter((a) => a.type === "event" && a.event.type === "agent_message")
      .map((a) => (a as Extract<ChatSessionAction, { type: "event" }>).event);
    expect(messages).toEqual([
      { type: "agent_message", text: "one", itemId: "i1" },
      { type: "agent_message", text: "two", itemId: "i2" },
      { type: "agent_message", text: "three", itemId: "i3" },
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
      setConfigOption: vi.fn(),
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
      useSessionConnection({
        ...hookOptions(),
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
