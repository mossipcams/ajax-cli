import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, waitFor, act, within } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";

vi.mock("@/features/runtime-control/useRuntimeControl", () => ({
  useRuntimeControl: vi.fn(() => ({
    status: null,
    loading: false,
    busy: false,
    overlay: null,
    error: null,
    dismissError: vi.fn(),
    confirmAction: null,
    updateAvailable: false,
    operationLabel: "idle",
    terminalResult: null,
    runRestart: vi.fn(),
    runUpdate: vi.fn(),
  })),
}));

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

describe("App connection status", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    vi.stubGlobal("WebSocket", class {
      readyState = 1;
      close() {}
      addEventListener() {}
      send() {}
    });
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

  it("defers the version check until the browser is idle", async () => {
    let idleCb: (() => void) | null = null;
    vi.stubGlobal("requestIdleCallback", (cb: () => void) => {
      idleCb = cb;
      return 1;
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const hitVersion = () =>
      fetchMock.mock.calls.some(([path]) => String(path) === "/api/version");

    expect(hitVersion()).toBe(false);
    expect(typeof idleCb).toBe("function");
    idleCb!();
    await vi.waitFor(() => expect(hitVersion()).toBe(true));
  });

  // iOS launches a home-screen PWA with the document still hidden behind the
  // splash screen. The mount load must go through anyway; only the repeating
  // background poll may skip while hidden.
  it("loads the cockpit on mount while hidden, but skips the background poll", async () => {
    vi.useFakeTimers();
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const cockpitCalls = () =>
      fetchMock.mock.calls.filter(([path]) => String(path) === "/api/cockpit").length;

    // Mount load is not swallowed by the hidden document.
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    // Hidden interval is 60s; firing it must not add a background fetch.
    await vi.advanceTimersByTimeAsync(120000);
    expect(cockpitCalls()).toBe(1);
  });

  it("retries a failed hidden PWA launch until the first cockpit projection loads", async () => {
    vi.useFakeTimers();
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    let cockpitCalls = 0;
    let releaseIntervalRetry: (() => void) | null = null;
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        if (cockpitCalls === 1) {
          return Promise.reject(new Error("network error"));
        }
        return new Promise<Response>((resolve) => {
          releaseIntervalRetry = () => resolve(jsonResponse(cockpit));
        });
      }
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const cockpitFetchCalls = () =>
      fetchMock.mock.calls.filter(([path]) => String(path) === "/api/cockpit").length;

    await vi.waitFor(() => expect(cockpitFetchCalls()).toBe(1));

    await vi.waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveAttribute(
        "data-state",
        "backend unreachable",
      ),
    );
    expect(cockpitFetchCalls()).toBe(1);

    await vi.advanceTimersByTimeAsync(3000);

    await vi.waitFor(() => expect(cockpitFetchCalls()).toBe(2));
    releaseIntervalRetry!();
    await vi.waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected"),
    );

    await vi.advanceTimersByTimeAsync(120000);
    expect(cockpitFetchCalls()).toBe(2);
  });

  it("timed-out cockpit GET releases polling for recovery", async () => {
    vi.useFakeTimers();
    vi.spyOn(AbortSignal, "timeout").mockImplementation((ms: number) => {
      const controller = new AbortController();
      setTimeout(() => {
        controller.abort(new DOMException("TimeoutError", "TimeoutError"));
      }, ms);
      return controller.signal;
    });
    let cockpitCalls = 0;
    const fetchMock = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        if (cockpitCalls === 1) {
          return new Promise<Response>((_resolve, reject) => {
            const signal = init?.signal;
            if (!signal) {
              reject(new Error("expected abort signal"));
              return;
            }
            const onAbort = () => {
              reject(signal.reason ?? new DOMException("Aborted", "AbortError"));
            };
            if (signal.aborted) {
              onAbort();
              return;
            }
            signal.addEventListener("abort", onAbort, { once: true });
          });
        }
        return Promise.resolve(jsonResponse(cockpit));
      }
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls).toBe(1));

    await act(async () => {
      // GET timeout fires at 10s; active cadence is 3s so the next poll lands at 12s.
      await vi.advanceTimersByTimeAsync(13_001);
      await Promise.resolve();
    });

    await vi.waitFor(() => expect(cockpitCalls).toBe(2));
    await vi.waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected"),
    );
    expect(screen.queryByText(/backend unreachable|disconnected|stale session/)).toBeNull();
  });

  it("reports reachable cockpit HTTP failures as disconnected", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 503,
        text: () => Promise.resolve("Service Unavailable"),
      }),
    );

    render(<App />);

    expect(await screen.findByText("disconnected: HTTP 503")).toBeInTheDocument();
    expect(screen.queryByText("backend unreachable")).toBeNull();
  });

  it("reports missing browser session as stale session", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        text: () => Promise.resolve(JSON.stringify({ ok: false, error: "browser session required" })),
      }),
    );

    render(<App />);

    expect(await screen.findByText("stale session: HTTP 401")).toBeInTheDocument();
    expect(screen.queryByText("disconnected: HTTP 401")).toBeNull();
  });

  it("recovers a missing browser session before showing stale session", async () => {
    let cockpitCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          return Promise.resolve(
            cockpitCalls === 1
              ? jsonResponse({ ok: false, error: "browser session required" }, 401)
              : jsonResponse(cockpit),
          );
        }
        if (path === "/api/session") return Promise.resolve(jsonResponse({ ok: true }));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);

    expect(await screen.findByText("connected")).toBeInTheDocument();
    expect(screen.queryByText("stale session")).toBeNull();
  });

  it("reports stale session when browser session renewal fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          return Promise.resolve(jsonResponse({ ok: false, error: "browser session required" }, 401));
        }
        if (path === "/api/session") {
          return Promise.resolve(jsonResponse({ ok: false, error: "renew failed" }, 503));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);

    expect(await screen.findByText("stale session: HTTP 503")).toBeInTheDocument();
    expect(screen.queryByText("connected")).toBeNull();
  });

  it("reports cockpit network failures as backend unreachable with detail", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Failed to fetch")));

    render(<App />);

    expect(await screen.findByText("backend unreachable: Failed to fetch")).toBeInTheDocument();
  });

  it("recovers from backend unreachable to connected when Retry succeeds", async () => {
    let cockpitCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          if (cockpitCalls === 1) {
            return Promise.reject(new Error("Failed to fetch"));
          }
          return Promise.resolve(jsonResponse(cockpit));
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);

    expect(await screen.findByText("backend unreachable: Failed to fetch")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected"),
    );
    expect(screen.queryByText("backend unreachable")).toBeNull();
  });

  it("reports reachable detail HTTP failures as disconnected", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        if (path.startsWith("/api/tasks/")) {
          return Promise.resolve(jsonResponse({ error: "detail unavailable" }, 500));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");

    expect(await screen.findByText("disconnected: HTTP 500")).toBeInTheDocument();
  });

  it("clears detail failure text after a later successful detail load", async () => {
    let detailCalls = 0;
    let resumeCalls = 0;
    let releaseCockpit!: (value: ReturnType<typeof jsonResponse>) => void;
    let releaseResume!: (value: ReturnType<typeof jsonResponse>) => void;
    const cockpitPending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      releaseCockpit = resolve;
    });
    const resumePending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      releaseResume = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return cockpitPending;
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/operations") {
          resumeCalls += 1;
          if (resumeCalls === 1) return resumePending;
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        if (path.startsWith("/api/tasks/")) {
          detailCalls += 1;
          // First open fails; reopen after leaving the task succeeds.
          if (detailCalls === 1) {
            return Promise.resolve(jsonResponse({ error: "detail unavailable" }, 500));
          }
          return Promise.resolve(jsonResponse(taskDetail));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    expect(await screen.findByText("disconnected: HTTP 500")).toBeInTheDocument();
    releaseResume(jsonResponse({ ok: true }));
    releaseCockpit(jsonResponse(cockpit));
    await waitFor(() => expect(true).toBe(true));

    // Flush the dashboard intermediate so the detail effect observes handle=null
    // before reopening the same task (sync double-hashchange would otherwise batch).
    setHash("#/");
    await waitFor(() => expect(true).toBe(true));
    setHash("#/t/web%2Ffix-login");

    expect(await screen.findByText("connected")).toBeInTheDocument();
    expect(screen.queryByText("disconnected: HTTP 500")).toBeNull();
  });

  it("renders task-load-error when detail fetch rejects and Retry refetches", async () => {
    let detailCalls = 0;
    let allowDetailSuccess = false;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) {
          detailCalls += 1;
          if (!allowDetailSuccess) {
            return Promise.resolve({
              ok: false,
              status: 503,
              text: () => Promise.resolve("Service unavailable"),
            });
          }
          return Promise.resolve(jsonResponse(taskDetail));
        }
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: false }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    expect(await screen.findByTestId("task-load-error")).toBeInTheDocument();
    expect(screen.getByText(/Could not load this task —/)).toBeInTheDocument();
    const callsBeforeRetry = detailCalls;

    allowDetailSuccess = true;
    fireEvent.click(
      within(screen.getByTestId("task-load-error")).getByRole("button", { name: "Retry" }),
    );
    await screen.findByText("Fix login");
    expect(detailCalls).toBe(callsBeforeRetry + 1);
  });
});
