import { renderHook, act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import { writeSessionModel } from "./sessionModel";
import { useSessionTransport } from "./useSessionTransport";

describe("useSessionTransport", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    localStorage.clear();
  });

  it("uses the current model preference after reconnecting", () => {
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
        setConnected: vi.fn(),
        setEverOpened: vi.fn(),
      }),
    );

    writeSessionModel("composer-2.5");
    act(() => callbacks[0]?.onClosed());
    act(() => vi.advanceTimersByTime(0));

    expect(models).toEqual(["auto", "composer-2.5"]);
    unmount();
  });
});
