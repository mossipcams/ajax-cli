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
    // Follow-up calls settle with a distinct state so tests can prove the
    // queued refetch (not just the first response) is what landed.
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
    // Flush any lingering poll interval so a leaked mount cannot hit the next test's mock.
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
    // The mount load is in flight when refresh() arrives (the post-action case).
    expect(countStatusCalls(fetchMock)).toBe(1);

    // Two refreshes race the same in-flight fetch: both queue, only one follows up.
    await act(async () => {
      await result.current.refresh();
    });
    await act(async () => {
      await result.current.refresh();
    });

    settleFirst(statusResponse("idle"));
    await act(async () => {});

    // Exactly one follow-up fired; a second queued one must not stack on it.
    expect(countStatusCalls(fetchMock)).toBe(2);
    expect(result.current.status?.state).toBe("serve");

    // Cadence resumes normally: the next 3s tick is the only additional call.
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

    // Refresh queued while the first fetch is in flight, then that fetch fails.
    await act(async () => {
      await result.current.refresh();
    });

    settleFirst(errorResponse("host unreachable"));
    await act(async () => {});

    // No follow-up: an unreachable host keeps its poll cadence instead of
    // immediately stacking requests on the failure.
    expect(countStatusCalls(fetchMock)).toBe(1);
    expect(result.current.error).toBe("host unreachable");
    expect(result.current.status).toBeNull();

    // The next scheduled poll takes over and lands fresh state.
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
    // Closed dialog: no status traffic at all, ever.
    await act(async () => {
      vi.advanceTimersByTime(9000);
    });
    expect(closedMock).not.toHaveBeenCalled();

    // Reopening the dialog starts one initial fetch plus the 3s cadence.
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
