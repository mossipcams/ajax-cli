import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, act } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { taskHash } from "@/shared/lib/routes";

// Hard file-scope stub: late microtasks must never reach jsdom's real WebSocket.
class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

function setHash(hash: string) {
  window.location.hash = hash;
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App polling cadence", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    vi.stubGlobal(
      "WebSocket",
      class {
        readyState = 1;
        close() {}
        addEventListener() {}
        send() {}
      },
    );
    vi.stubGlobal(
      "ResizeObserver",
      class MockResizeObserver {
        observe = vi.fn();
        disconnect = vi.fn();
      },
    );
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  // Polling-cadence lifecycle. These pin the behaviour that the two
  // `react-hooks/exhaustive-deps` suppressions used to hide: the interval effect
  // must reschedule on cadence change, must not churn on unrelated re-renders,
  // and the mount-once listener effect must stay subscribed exactly once.
  function cockpitCountingFetch() {
    let cockpitCalls = 0;
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        return Promise.resolve(jsonResponse(cockpit));
      }
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
      if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
      if (path === "/api/operations") return Promise.resolve(jsonResponse({}));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    return { fetchMock, cockpitCalls: () => cockpitCalls };
  }

  it("polls the cockpit on the dashboard cadence", async () => {
    vi.useFakeTimers();
    const { fetchMock, cockpitCalls } = cockpitCountingFetch();
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    // Dashboard cadence is 3000ms: three ticks add three polls.
    await vi.advanceTimersByTimeAsync(9000);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(4));
  });

  it("polls the cockpit on the idle cadence when every card is idle", async () => {
    vi.useFakeTimers();
    const quietCockpit = {
      ...cockpit,
      cards: cockpit.cards.map((card) => ({ ...card, status: "idle" })),
    };
    let cockpitCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          return Promise.resolve(jsonResponse(quietCockpit));
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({}));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls).toBe(1));
    // Flush the quiet-fleet re-render so the 10s idle interval replaces the
    // 3s active one started while cockpit.data was still null.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    // Quiet fleet cadence is 10s: 3s active would add a poll here.
    await vi.advanceTimersByTimeAsync(3000);
    expect(cockpitCalls).toBe(1);

    await vi.advanceTimersByTimeAsync(7000);
    await vi.waitFor(() => expect(cockpitCalls).toBe(2));
  });

  it("reschedules the cockpit interval when the route cadence changes", async () => {
    vi.useFakeTimers();
    const { fetchMock, cockpitCalls } = cockpitCountingFetch();
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    // Task route slows the cadence to 10000ms. If the old 3000ms interval were
    // left running, 4000ms would add one poll instead of none.
    await act(async () => {
      setHash(taskHash("web/a"));
    });
    // Guard: a wrong prefix would silently leave the route on dashboard and the
    // 3000ms cadence would look correct.
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
    const afterRouteChange = cockpitCalls();

    await vi.advanceTimersByTimeAsync(9000);
    expect(cockpitCalls()).toBe(afterRouteChange);

    await vi.advanceTimersByTimeAsync(1000);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(afterRouteChange + 1));
  });

  it("refreshes the cockpit when returning from a failed task route", async () => {
    vi.useFakeTimers();
    let cockpitCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          return Promise.resolve(jsonResponse(cockpit));
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse({}, 404));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({}));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls).toBe(1));

    await act(async () => {
      setHash(taskHash("web/missing"));
    });
    // #861: a missing-task 404 is a detail error, not a cockpit disconnect.
    await vi.waitFor(() => expect(screen.getByTestId("task-load-error")).toBeInTheDocument());
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected");

    const beforeReturn = cockpitCalls;
    await act(async () => {
      setHash("");
    });
    await vi.waitFor(() => expect(cockpitCalls).toBeGreaterThan(beforeReturn));
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected");
  });

  it("keeps one focus listener across re-renders", async () => {
    vi.useFakeTimers();
    const { fetchMock, cockpitCalls } = cockpitCountingFetch();
    vi.stubGlobal("fetch", fetchMock);
    const addSpy = vi.spyOn(window, "addEventListener");

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    const focusRegistrations = addSpy.mock.calls.filter(([type]) => type === "focus").length;
    expect(focusRegistrations).toBe(1);

    // A focus resume triggers exactly one extra cockpit load after debounce, not
    // one per re-render that has happened since mount.
    const beforeFocus = cockpitCalls();
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(750);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(beforeFocus + 1));
    await vi.advanceTimersByTimeAsync(0);
    expect(cockpitCalls()).toBe(beforeFocus + 1);
  });

  it("coalesces overlapping shell recovery signals into one trailing cockpit load", async () => {
    vi.useFakeTimers();
    let cockpitCalls = 0;
    let rejectFirst!: (reason?: unknown) => void;
    let resolveSecond!: (value: ReturnType<typeof jsonResponse>) => void;
    const firstPending = new Promise<ReturnType<typeof jsonResponse>>((_, reject) => {
      rejectFirst = reject;
    });
    const secondPending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      resolveSecond = resolve;
    });

    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          if (cockpitCalls === 1) return firstPending;
          if (cockpitCalls === 2) return secondPending;
          return Promise.reject(new Error(`unexpected extra cockpit call: ${cockpitCalls}`));
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({}));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(cockpitCalls).toBe(1);

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      window.dispatchEvent(new Event("pageshow"));
      window.dispatchEvent(new Event("online"));
      Object.defineProperty(document, "visibilityState", {
        configurable: true,
        value: "visible",
      });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(cockpitCalls).toBe(1);

    await vi.advanceTimersByTimeAsync(750);

    await act(async () => {
      rejectFirst(new Error("network error"));
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(cockpitCalls).toBe(2);

    await act(async () => {
      resolveSecond(jsonResponse(cockpit));
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    await vi.waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected"),
    );
    expect(cockpitCalls).toBe(2);
  });

  // Regression: loadDetail must not depend on cockpit data. It is a dependency
  // of the detail effect, so an identity that churns with each poll re-runs that
  // effect and fires an extra resume mutation every time the projection changes.
  // A static fixture hides this — the apply gate suppresses unchanged
  // projections — so this drives a cockpit whose payload really does change.
  it("does not re-resume an open task when the cockpit projection changes", async () => {
    let cockpitCalls = 0;
    let resumeCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          // Each poll returns a genuinely different projection.
          return Promise.resolve(
            jsonResponse({
              ...cockpit,
              cards: cockpit.cards.map((card, index) =>
                index === 0 ? { ...card, title: `Changed ${cockpitCalls}` } : card,
              ),
            }),
          );
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") {
          const body = String(init?.body ?? "");
          if (body.includes('"resume"')) resumeCalls += 1;
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    vi.useFakeTimers();
    render(<App />);
    await act(async () => {
      setHash(taskHash("web/a"));
    });
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
    await vi.waitFor(() => expect(resumeCalls).toBe(1));

    // Task-route cadence is 10000ms; drive polls, each with a changed payload.
    const pollsAtStart = cockpitCalls;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(cockpitCalls).toBeGreaterThan(pollsAtStart);

    // Changed projections must not add resume mutations.
    expect(resumeCalls).toBe(1);
  });

  it("removes shell listeners on unmount", async () => {
    vi.useFakeTimers();
    const { fetchMock, cockpitCalls } = cockpitCountingFetch();
    vi.stubGlobal("fetch", fetchMock);

    const { unmount } = render(<App />);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    unmount();
    const afterUnmount = cockpitCalls();
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(5000);
    expect(cockpitCalls()).toBe(afterUnmount);
  });
});
