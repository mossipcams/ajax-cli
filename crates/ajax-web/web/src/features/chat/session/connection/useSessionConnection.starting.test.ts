import { renderHook, act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "../transport/public";
import {
  connectionStateAllowsSend,
  initialConnectionState,
  type ConnectionState,
} from "./connectionState";
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

describe("useSessionConnection starting state", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
  });

  it("moves to starting on an agent-pending snapshot and to connected on the live ready", () => {
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
    const states: ConnectionState[] = [];
    const setConnectionState = vi.fn((state: ConnectionState) => states.push(state));

    const { unmount } = renderHook(() =>
      useSessionConnection(hookOptions({ setConnectionState })),
    );

    act(() => callbacks[0]?.onAgentPending?.());
    expect(states).toEqual(["connecting", "starting"]);
    expect(connectionStateAllowsSend("starting")).toBe(false);

    act(() => callbacks[0]?.onReady("auto"));
    expect(states).toEqual(["connecting", "starting", "connected"]);
    expect(connectionStateAllowsSend("connected")).toBe(true);
    unmount();
  });
});
