import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import App from "./App.svelte";
import cockpit from "../fixtures/cockpit.json";
import taskDetail from "../fixtures/task-detail.json";

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn();
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock("xterm-zerolag-input", () => ({
  ZerolagInputAddon: class MockZerolagInputAddon {
    getFlushed = vi.fn(() => ({ count: 0, text: "" }));
    setFlushed = vi.fn();
    removeChar = vi.fn();
    clear = vi.fn();
    clearFlushed = vi.fn();
    rerender = vi.fn();
    dispose = vi.fn();
  },
}));

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

describe("App shell", () => {
  beforeEach(() => {
    window.location.hash = "";
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
    vi.unstubAllGlobals();
  });

  it("renders the shared chrome", () => {
    const { getByRole, container } = render(App);
    expect(getByRole("heading", { name: "Ajax" })).toBeInTheDocument();
    expect(container.querySelector(".connection-status")).toBeInTheDocument();
    expect(container.querySelector(".update-banner")).toBeInTheDocument();
    expect(container.querySelector(".bottom-nav")).toBeInTheDocument();
    expect(container.querySelector("[data-bottom-action='new-task']")).toBeInTheDocument();
    expect(container.querySelector("main")).toBeInTheDocument();
  });

  it("shows the dashboard outlet by default", () => {
    const { container } = render(App);
    expect(container.querySelector("[data-outlet='dashboard']")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='settings']")).toBeNull();
  });

  it("shows a dashboard skeleton while the cockpit projection is loading", () => {
    const { container } = render(App);
    expect(container.querySelector("[data-testid='dashboard-skeleton']")).toBeInTheDocument();
    expect(container.querySelector(".empty")).toBeNull();
  });

  it("shows a task skeleton while a task detail is loading", async () => {
    const { container, findByTestId } = render(App);
    setHash("#/t/web%2Ffix-login");
    await findByTestId("outlet-task");
    expect(container.querySelector("[data-testid='task-skeleton']")).toBeInTheDocument();
  });

  it("shows the settings outlet on the settings route", async () => {
    const { container, findByTestId } = render(App);
    setHash("#/settings");
    expect(await findByTestId("outlet-settings")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='dashboard']")).toBeNull();
  });

  it("shows the task outlet on a task route", async () => {
    const { findByTestId } = render(App);
    setHash("#/t/web%2Ffix-login");
    expect(await findByTestId("outlet-task")).toBeInTheDocument();
  });

  it("mounts the task terminal panel after detail loads", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return Promise.resolve(jsonResponse(taskDetail));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    vi.stubGlobal("WebSocket", class {
      readyState = 1;
      close() {}
      addEventListener() {}
      send() {}
    });

    const { findByTestId } = render(App);
    setHash("#/t/web%2Ffix-login");

    expect(await findByTestId("task-terminal-panel")).toBeInTheDocument();
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

    render(App);

    const hitVersion = () =>
      fetchMock.mock.calls.some(([path]) => String(path) === "/api/version");

    expect(hitVersion()).toBe(false);
    expect(typeof idleCb).toBe("function");
    idleCb!();
    await vi.waitFor(() => expect(hitVersion()).toBe(true));
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

    const { findByText, queryByText } = render(App);

    expect(await findByText("disconnected: HTTP 503")).toBeInTheDocument();
    expect(queryByText("backend unreachable")).toBeNull();
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

    const { findByText, queryByText } = render(App);

    expect(await findByText("stale session: HTTP 401")).toBeInTheDocument();
    expect(queryByText("disconnected: HTTP 401")).toBeNull();
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

    const { findByText, queryByText } = render(App);

    expect(await findByText("connected")).toBeInTheDocument();
    expect(queryByText("stale session")).toBeNull();
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

    const { findByText, queryByText } = render(App);

    expect(await findByText("stale session: HTTP 503")).toBeInTheDocument();
    expect(queryByText("connected")).toBeNull();
  });

  it("reports cockpit network failures as backend unreachable with detail", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Failed to fetch")));

    const { findByText } = render(App);

    expect(await findByText("backend unreachable: Failed to fetch")).toBeInTheDocument();
  });

  it("reports reachable detail HTTP failures as disconnected", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) {
          return Promise.resolve(jsonResponse({ error: "detail unavailable" }, 500));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { findByText } = render(App);
    setHash("#/t/web%2Ffix-login");

    expect(await findByText("disconnected: HTTP 500")).toBeInTheDocument();
  });

  it("clears detail failure text after a later successful detail load", async () => {
    let detailCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) {
          detailCalls += 1;
          if (detailCalls <= 2) {
            return Promise.resolve(jsonResponse({ error: "detail unavailable" }, 500));
          }
          return Promise.resolve(jsonResponse(taskDetail));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { findByText, queryByText } = render(App);
    setHash("#/t/web%2Ffix-login");
    expect(await findByText("disconnected: HTTP 500")).toBeInTheDocument();

    setHash("#/");
    setHash("#/t/web%2Ffix-login");

    expect(await findByText("connected")).toBeInTheDocument();
    expect(queryByText("disconnected: HTTP 500")).toBeNull();
  });
});
