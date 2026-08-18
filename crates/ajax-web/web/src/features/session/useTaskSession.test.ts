import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import {
  SESSION_MODEL_STORAGE_KEY,
  writeSessionModel,
} from "./sessionModel";
import { useTaskSession } from "./useTaskSession";

describe("useTaskSession", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
    localStorage.clear();
  });

  beforeEach(() => {
    localStorage.clear();
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

  // Regression for issue #931: the in-session picker must track the host
  // snapshot, not the browser localStorage preference, and must not revert
  // while a set_model change is still pending.
  it("keeps an in-session model change when a stale host snapshot arrives (#931)", () => {
    writeSessionModel("composer-2.5");
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

    const { result, unmount } = renderHook(() =>
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => callbacks[0]?.onReady("gpt-5.6-sol-medium"));
    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");

    act(() => result.current.setModel("claude-opus-5"));
    expect(result.current.sessionModel).toBe("claude-opus-5");
    expect(transport.setModel).toHaveBeenCalledWith("claude-opus-5");

    act(() => callbacks[0]?.onReady("gpt-5.6-sol-medium"));
    expect(result.current.sessionModel).toBe("claude-opus-5");

    act(() => callbacks[0]?.onReady("claude-opus-5"));
    expect(result.current.sessionModel).toBe("claude-opus-5");
    expect(localStorage.getItem(SESSION_MODEL_STORAGE_KEY)).toBe("claude-opus-5");
    unmount();
  });

  it("reverts an optimistic model change when the host reports an error (#931)", () => {
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
        nextCallbacks.onReady("gpt-5.6-sol-medium");
        return transport;
      },
    );

    const { result, unmount } = renderHook(() =>
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => result.current.setModel("claude-opus-5"));
    expect(result.current.sessionModel).toBe("claude-opus-5");

    act(() => callbacks[0]?.onEvent({ type: "error", message: "session model change needs a task Ajax started over ACP" }));
    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");
    unmount();
  });

  // Regression for issue #942: unrelated prompt errors must not snap the picker
  // back after the host confirms the new model on a generation-changed snapshot.
  it("keeps a confirmed model change when the first prompt emits an unrelated error (#942)", () => {
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
        nextCallbacks.onReady("gpt-5.6-sol-medium");
        return transport;
      },
    );

    const { result, unmount } = renderHook(() =>
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => result.current.setModel("claude-opus-5"));
    act(() => callbacks[0]?.onReady("claude-opus-5"));
    expect(result.current.sessionModel).toBe("claude-opus-5");

    act(() =>
      callbacks[0]?.onEvent({ type: "error", message: "ACP process exited" }),
    );
    expect(result.current.sessionModel).toBe("claude-opus-5");
    unmount();
  });

  it("does not seed the in-session picker from localStorage (#931)", () => {
    writeSessionModel("composer-2.5");
    const transport: webSessionTransport.WebSessionTransport = {
      sendPrompt: vi.fn(() => "prompt-1"),
      sendCancel: vi.fn(),
      setModel: vi.fn(),
      respondPermission: vi.fn(),
      dispose: vi.fn(),
    };
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady("gpt-5.6-sol-medium");
        return transport;
      },
    );

    const { result, unmount } = renderHook(() =>
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");
    expect(result.current.sessionModel).not.toBe("composer-2.5");
    unmount();
  });

  // Regression for #952: the in-session picker tracks the host snapshot applied model,
  // not task metadata or localStorage, when the harness reports a different id.
  it("binds the in-session picker to the host snapshot applied model (#952)", () => {
    writeSessionModel("composer-2.5");
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

    const { result, unmount } = renderHook(() =>
      useTaskSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => callbacks[0]?.onReady("harness-default"));
    expect(result.current.sessionModel).toBe("harness-default");
    expect(result.current.sessionModel).not.toBe("composer-2.5");
    unmount();
  });
});
