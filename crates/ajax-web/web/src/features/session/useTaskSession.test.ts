import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import { useTaskSession } from "./useTaskSession";

describe("useTaskSession", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
  });

  it("clears the session outbox when the task mutates", () => {
    sessionStorage.setItem(
      "ajax.web.session.outbox.web%2Ffix-login",
      JSON.stringify([{ text: "queued", clientMessageId: "c1" }]),
    );
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady("auto");
        return transport;
      },
    );

    const onMutated = vi.fn();
    const { result, unmount } = renderHook(() =>
      useTaskSession({
        handle: "web/fix-login",
        detail: null,
        onMutated,
      }),
    );

    act(() => result.current.onMutated());

    expect(sessionStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();
    expect(onMutated).toHaveBeenCalledOnce();
    unmount();
  });

  it("clears the session outbox when the session identity is invalid", () => {
    vi.useFakeTimers();
    sessionStorage.setItem(
      "ajax.web.session.outbox.web%2Ffix-login",
      JSON.stringify([{ text: "queued", clientMessageId: "c1" }]),
    );
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
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    for (let attempt = 0; attempt < 6; attempt += 1) {
      act(() => callbacks.at(-1)?.onClosed());
      act(() => vi.advanceTimersByTime(0));
    }

    expect(sessionStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();
    unmount();
    vi.useRealTimers();
  });
});
