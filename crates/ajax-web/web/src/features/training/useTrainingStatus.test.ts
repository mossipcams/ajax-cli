import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useTrainingStatus } from "./useTrainingStatus";

function statusResponse(state: string): Response {
  return new Response(
    JSON.stringify({
      ok: true,
      state,
      runtime_up: true,
      active_profile: null,
      run: null,
      generation: null,
      profile_details: null,
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

function errorResponse(message: string): Response {
  return new Response(JSON.stringify({ ok: false, error: message }), {
    status: 502,
    headers: { "content-type": "application/json" },
  });
}

type FetchMock = ReturnType<typeof vi.fn>;

function countStatusCalls(fetchMock: FetchMock): number {
  return fetchMock.mock.calls.filter((call) => String(call[0]).includes("/status"))
    .length;
}

function stubStatusFetch(
  firstResponse?: Promise<Response>,
  followUpState = "serve",
): FetchMock {
  let statusCalls = 0;
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    if (!String(input).includes("/api/training/status")) {
      throw new Error(`unmocked route ${String(input)}`);
    }
    statusCalls += 1;
    return statusCalls === 1 && firstResponse ? firstResponse : Promise.resolve(statusResponse(followUpState));
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("useTrainingStatus", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("fires at most one queued refetch when refreshes arrive during an in-flight success", async () => {
    let settleFirst: (response: Response) => void = () => undefined;
    const firstInFlight = new Promise<Response>((resolve) => {
      settleFirst = resolve;
    });
    const fetchMock = stubStatusFetch(firstInFlight);

    const { result } = renderHook(() => useTrainingStatus(true));
    expect(countStatusCalls(fetchMock)).toBe(1);

    await act(async () => {
      await result.current.refresh();
    });
    await act(async () => {
      await result.current.refresh();
    });

    settleFirst(statusResponse("idle"));
    await act(async () => {});

    expect(countStatusCalls(fetchMock)).toBe(2);
    expect(result.current.status?.state).toBe("serve");

    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(countStatusCalls(fetchMock)).toBe(3);
  });

  it("queues no follow-up refresh when the raced fetch fails", async () => {
    let settleFirst: (response: Response) => void = () => undefined;
    const firstInFlight = new Promise<Response>((resolve) => {
      settleFirst = resolve;
    });
    const fetchMock = stubStatusFetch(firstInFlight);

    const { result } = renderHook(() => useTrainingStatus(true));
    expect(countStatusCalls(fetchMock)).toBe(1);

    await act(async () => {
      await result.current.refresh();
    });

    settleFirst(errorResponse("host unreachable"));
    await act(async () => {});

    expect(countStatusCalls(fetchMock)).toBe(1);
    expect(result.current.error).toBe("host unreachable");
    expect(result.current.status).toBeNull();

    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(countStatusCalls(fetchMock)).toBe(2);
    expect(result.current.error).toBeNull();
    expect(result.current.status?.state).toBe("serve");
  });

  it("polls on a 3s cadence only while enabled", async () => {
    const closedMock = vi.fn(async (input: RequestInfo | URL) => {
      if (!String(input).includes("/api/training/status")) {
        throw new Error(`unmocked route ${String(input)}`);
      }
      return statusResponse("idle");
    });
    vi.stubGlobal("fetch", closedMock);

    renderHook(() => useTrainingStatus(false));
    await act(async () => {
      vi.advanceTimersByTime(9000);
    });
    expect(closedMock).not.toHaveBeenCalled();

    const openMock = stubStatusFetch(undefined, "idle");
    const { result } = renderHook(() => useTrainingStatus(true));
    await act(async () => {});
    expect(countStatusCalls(openMock)).toBe(1);

    await act(async () => {
      vi.advanceTimersByTime(6000);
    });
    expect(countStatusCalls(openMock)).toBe(3);
    expect(result.current.status?.state).toBe("idle");
  });
});
