import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useEarlierHistory } from "./useEarlierHistory";

function page(from = 10) {
  return {
    protocolVersion: 2,
    from,
    floor: 0,
    events: [
      { cursor: from, payload: { type: "message", role: "user", text: "Older", itemId: "old" } },
      { cursor: from + 1, payload: { type: "artifact", kind: "file", title: "Old artifact" } },
      { cursor: from + 2, payload: { type: "error", message: "Old restore failure" } },
    ].map((event) => ({ type: "event", protocolVersion: 2, ...event })),
  };
}

function response(body = page()) {
  return new Response(JSON.stringify(body));
}

function deferredFetch() {
  let resolve!: (value: Response) => void;
  const fetchImpl = vi.fn<typeof fetch>().mockImplementation(() => new Promise<Response>((done) => {
    resolve = done;
  }));
  return { fetchImpl, resolve: (value = response()) => resolve(value) };
}

function setup(fetchImpl: typeof fetch, hasEarlierLocally = false) {
  const dispatch = vi.fn();
  const revealEarlier = vi.fn(() => 12);
  const hook = renderHook((props) => useEarlierHistory({
    ...props, dispatch, revealEarlier, fetchImpl,
  }), {
    initialProps: {
      handle: "repo/task",
      serverHistory: { from: 100, floor: 0 } as { from: number; floor: number } | undefined,
      hasEarlierLocally,
    },
  });
  return { ...hook, dispatch, revealEarlier };
}

describe("useEarlierHistory", () => {
  it("fetches before the server bound and dispatches the projected prepend page", async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(response());
    const { result, dispatch } = setup(fetchImpl);
    expect(result.current.hasEarlier).toBe(true);
    act(() => expect(result.current.loadEarlier()).toBe(0));
    expect(fetchImpl).toHaveBeenCalledExactlyOnceWith(
      "/api/tasks/repo%2Ftask/session/transcript?before=100",
      { method: "GET", signal: expect.any(AbortSignal) },
    );
    await waitFor(() => expect(dispatch).toHaveBeenCalledExactlyOnceWith({
      type: "event",
      event: {
        type: "prepend",
        events: [
          { type: "user_message", text: "Older", itemId: "old", messageId: undefined },
          { type: "session_error", message: "Old restore failure" },
        ],
        from: 10,
      },
    }));
    expect(result.current.error).toBe(false);
  });

  it("guards repeated clicks while a fetch is in flight", async () => {
    const pending = deferredFetch();
    const { result, dispatch } = setup(pending.fetchImpl);
    act(() => {
      expect(result.current.loadEarlier()).toBe(0);
      expect(result.current.loadEarlier()).toBe(0);
      expect(result.current.loadEarlier()).toBe(0);
    });
    expect(pending.fetchImpl).toHaveBeenCalledOnce();
    await act(async () => pending.resolve());
    expect(dispatch).toHaveBeenCalledOnce();
  });

  it("keeps history available after failure and clears the retry flag on retry", async () => {
    const fetchImpl = vi.fn<typeof fetch>()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(response());
    const { result, dispatch } = setup(fetchImpl);
    act(() => expect(result.current.loadEarlier()).toBe(0));
    await waitFor(() => expect(result.current.error).toBe(true));
    expect(result.current.hasEarlier).toBe(true);
    expect(dispatch).not.toHaveBeenCalled();
    act(() => expect(result.current.loadEarlier()).toBe(0));
    expect(result.current.error).toBe(false);
    await waitFor(() => expect(dispatch).toHaveBeenCalledOnce());
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it.each([100, 101])("ignores a duplicate or stale page with from=%s", async (from) => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(response({ ...page(from), events: [] }));
    const { result, dispatch } = setup(fetchImpl);
    await act(async () => expect(result.current.loadEarlier()).toBe(0));
    expect(fetchImpl).toHaveBeenCalledOnce();
    expect(dispatch).not.toHaveBeenCalled();
    expect(result.current.error).toBe(false);
    expect(result.current.hasEarlier).toBe(true);
  });

  it("compares a late page against the current server bound", async () => {
    const pending = deferredFetch();
    const { result, dispatch, rerender } = setup(pending.fetchImpl);
    act(() => result.current.loadEarlier());
    rerender({ handle: "repo/task", serverHistory: { from: 5, floor: 0 }, hasEarlierLocally: false });
    await act(async () => pending.resolve());
    expect(dispatch).not.toHaveBeenCalled();
  });

  it.each([
    { from: 200, floor: 20 },
    { from: 200, floor: 0 },
    { from: 100, floor: 20 },
  ])("discards a late page after bounds change to $from/$floor and allows retry", async (serverHistory) => {
    const pending = deferredFetch();
    const { result, dispatch, rerender } = setup(pending.fetchImpl);
    act(() => result.current.loadEarlier());
    rerender({ handle: "repo/task", serverHistory, hasEarlierLocally: false });
    await act(async () => pending.resolve());
    expect(dispatch).not.toHaveBeenCalled();
    expect(result.current.error).toBe(false);

    act(() => result.current.loadEarlier());
    expect(pending.fetchImpl).toHaveBeenCalledTimes(2);
    expect(pending.fetchImpl).toHaveBeenLastCalledWith(
      `/api/tasks/repo%2Ftask/session/transcript?before=${serverHistory.from}`,
      { method: "GET", signal: expect.any(AbortSignal) },
    );
    await act(async () => pending.resolve(response(page(50))));
    expect(dispatch).toHaveBeenCalledOnce();
    expect(result.current.error).toBe(false);
  });

  it("applies a late page after rerendering with the same bounds", async () => {
    const pending = deferredFetch();
    const { result, dispatch, rerender } = setup(pending.fetchImpl);
    act(() => result.current.loadEarlier());
    rerender({ handle: "repo/task", serverHistory: { from: 100, floor: 0 }, hasEarlierLocally: false });
    await act(async () => pending.resolve());
    expect(dispatch).toHaveBeenCalledExactlyOnceWith({
      type: "event",
      event: {
        type: "prepend",
        events: [
          { type: "user_message", text: "Older", itemId: "old", messageId: undefined },
          { type: "session_error", message: "Old restore failure" },
        ],
        from: 10,
      },
    });
    expect(result.current.error).toBe(false);
  });

  it("aborts on unmount and discards a late successful response", async () => {
    const pending = deferredFetch();
    const { result, dispatch, unmount } = setup(pending.fetchImpl);
    act(() => result.current.loadEarlier());
    const signal = pending.fetchImpl.mock.calls[0]![1]!.signal!;
    expect(signal.aborted).toBe(false);
    unmount();
    expect(signal.aborted).toBe(true);
    await act(async () => pending.resolve());
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("aborts on handle change without letting the old response clear the new guard", async () => {
    const oldRequest = deferredFetch();
    const newRequest = deferredFetch();
    const fetchImpl = vi.fn<typeof fetch>()
      .mockImplementationOnce(oldRequest.fetchImpl)
      .mockImplementationOnce(newRequest.fetchImpl);
    const { result, dispatch, rerender } = setup(fetchImpl);
    act(() => result.current.loadEarlier());
    const oldSignal = fetchImpl.mock.calls[0]![1]!.signal!;
    rerender({ handle: "repo/other", serverHistory: { from: 100, floor: 0 }, hasEarlierLocally: false });
    expect(oldSignal.aborted).toBe(true);
    act(() => result.current.loadEarlier());
    expect(fetchImpl).toHaveBeenCalledTimes(2);
    expect(fetchImpl.mock.calls[1]![0]).toContain("repo%2Fother");
    await act(async () => oldRequest.resolve());
    expect(dispatch).not.toHaveBeenCalled();
    act(() => result.current.loadEarlier());
    expect(fetchImpl).toHaveBeenCalledTimes(2);
    await act(async () => newRequest.resolve());
    expect(dispatch).toHaveBeenCalledOnce();
    expect(result.current.error).toBe(false);
  });

  it("keeps local reveal synchronous and does not fetch", () => {
    const fetchImpl = vi.fn<typeof fetch>();
    const { result, revealEarlier, dispatch } = setup(fetchImpl, true);
    expect(result.current.hasEarlier).toBe(true);
    act(() => expect(result.current.loadEarlier()).toBe(12));
    expect(revealEarlier).toHaveBeenCalledOnce();
    expect(fetchImpl).not.toHaveBeenCalled();
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("does nothing when both local and server history are exhausted", () => {
    const fetchImpl = vi.fn<typeof fetch>();
    const { result, rerender } = setup(fetchImpl);
    rerender({ handle: "repo/task", serverHistory: { from: 3, floor: 3 }, hasEarlierLocally: false });
    expect(result.current.hasEarlier).toBe(false);
    act(() => expect(result.current.loadEarlier()).toBe(0));
    rerender({ handle: "repo/task", serverHistory: undefined, hasEarlierLocally: false });
    expect(result.current.hasEarlier).toBe(false);
    act(() => expect(result.current.loadEarlier()).toBe(0));
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});
