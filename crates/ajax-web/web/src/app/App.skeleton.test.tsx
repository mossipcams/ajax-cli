import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

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

describe("App shell skeletons", () => {
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
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    window.location.hash = "";
    document.title = "";
  });

  it("shows a dashboard skeleton while the cockpit projection is loading", () => {
    render(<App />);
    expect(screen.getByTestId("dashboard-skeleton")).toBeInTheDocument();
    expect(screen.queryByText(/All quiet|No tasks/)).not.toBeInTheDocument();
  });

  it("shows a task skeleton while a task detail is loading", async () => {
    const hang = new Promise(() => {});
    vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/tasks/")) return hang;
      if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }));
    render(<App />);
    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");
    expect(screen.getByTestId("task-skeleton")).toBeInTheDocument();
  });

  it("#908: cold start on a missing task keeps main on skeleton until detail 404s", async () => {
    window.location.hash = "#/t/web%2Fmissing";
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (
          path.startsWith("/api/tasks/") &&
          !path.includes("/pull-requests") &&
          !path.includes("/diff")
        ) {
          return Promise.resolve(jsonResponse({}, 404));
        }
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: false }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    render(<App />);
    const main = screen.getByTestId("app-main");
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
    expect(main).toContainElement(screen.getByTestId("task-skeleton"));
    expect(screen.queryByTestId("task-load-error")).not.toBeInTheDocument();
    await screen.findByTestId("task-load-error");
    expect(screen.getByText(/Could not load this task/)).toBeInTheDocument();
    expect(screen.queryByTestId("task-skeleton")).not.toBeInTheDocument();
  });

  it("#908: navigating from dashboard before detail settles paints task skeleton in main", async () => {
    let resolveDetail!: (value: ReturnType<typeof jsonResponse>) => void;
    const detailPending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      resolveDetail = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (
          path.startsWith("/api/tasks/") &&
          !path.includes("/pull-requests") &&
          !path.includes("/diff")
        ) {
          return detailPending;
        }
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: false }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    render(<App />);
    await screen.findByTestId("outlet-dashboard");
    setHash("#/t/web%2Fmissing");
    const main = screen.getByTestId("app-main");
    await screen.findByTestId("outlet-task");
    expect(main).toContainElement(screen.getByTestId("task-skeleton"));
    expect(screen.queryByTestId("dashboard-skeleton")).not.toBeInTheDocument();
    await act(async () => {
      resolveDetail(jsonResponse({}, 404));
    });
    await screen.findByTestId("task-load-error");
  });

  it("#860: missing-task diff shows skeleton then error without DiffReview chrome", async () => {
    let resolveDetail!: (value: ReturnType<typeof jsonResponse>) => void;
    const detailPending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      resolveDetail = resolve;
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.includes("/pull-requests") || path.includes("/diff")) {
        return Promise.reject(new Error(`unexpected diff fetch: ${path}`));
      }
      if (path.startsWith("/api/tasks/")) return detailPending;
      if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: false }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    setHash("#/t/web%2Fmissing/diff/");
    await screen.findByTestId("outlet-diff");
    expect(screen.getByTestId("task-skeleton")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-review")).not.toBeInTheDocument();
    expect(screen.queryByText("Loading pull requests…")).not.toBeInTheDocument();
    await act(async () => {
      resolveDetail(jsonResponse({}, 404));
    });
    await screen.findByTestId("task-load-error");
    expect(screen.queryByTestId("diff-review")).not.toBeInTheDocument();
    expect(
      fetchMock.mock.calls.filter(([url]) => String(url).includes("/pull-requests")),
    ).toHaveLength(0);
    expect(
      fetchMock.mock.calls.filter(([url]) => String(url).includes("/diff")),
    ).toHaveLength(0);
  });
});
