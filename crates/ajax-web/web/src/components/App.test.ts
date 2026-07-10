import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { tick } from "svelte";
import App from "./App.svelte";
import appSource from "./App.svelte?raw";
import appViewportSource from "./AppViewport.svelte?raw";

function loadStylesSource(): string {
  const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
  return readFileSync(join(testDir, "../styles.css"), "utf8");
}
import cockpit from "../fixtures/cockpit.json";
import taskDetail from "../fixtures/task-detail.json";

vi.mock("ghostty-web", () => ({
  Ghostty: {
    load: vi.fn(() => Promise.resolve({ runtime: "ghostty" })),
  },
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    buffer = { active: { viewportY: 0, baseY: 0 } };
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    onScroll = vi.fn(() => ({ dispose: vi.fn() }));
    scrollToBottom = vi.fn();
    scrollLines = vi.fn();
    focus = vi.fn();
    blur = vi.fn();
    paste = vi.fn();
    resize = vi.fn();
    getViewportY = vi.fn(() => 0);
    options = { fontSize: 13 };
  },
  FitAddon: class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
    proposeDimensions = vi.fn(() => ({ cols: 80, rows: 24 }));
  },
}));

// Hard file-scope stub: late microtasks (detail loads settling between a
// test's unstubAllGlobals and DOM cleanup) must never reach jsdom's real
// WebSocket, whose `ws` shim rejects asynchronously outside any test.
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

describe("App shell", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
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

  it("renders the shared chrome", () => {
    const { getByRole, container } = render(App);
    expect(getByRole("heading", { name: "Ajax" })).toBeInTheDocument();
    expect(container.querySelector(".connection-status")).toBeInTheDocument();
    expect(container.querySelector(".update-banner")).toBeInTheDocument();
    expect(container.querySelector(".bottom-nav")).toBeInTheDocument();
    expect(container.querySelector("[data-bottom-action='new-task']")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='app-main']")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='route-scroll']")).toBeInTheDocument();
  });

  it("syncs --app-height from the visual viewport on mount", () => {
    vi.stubGlobal("visualViewport", {
      height: 712,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    document.documentElement.style.removeProperty("--app-height");
    render(App);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("712px");
  });

  it("shows the dashboard outlet by default", () => {
    const { container } = render(App);
    expect(container.querySelector("[data-outlet='dashboard']")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='settings']")).toBeNull();
  });

  it("exposes layout primitives for viewport and scroll ownership", () => {
    const { container } = render(App);
    expect(container.querySelector("[data-testid='app-viewport']")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='app-shell']")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='app-main']")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='route-scroll']")).toBeInTheDocument();
    expect(appSource).not.toMatch(/initViewport/);
    expect(appViewportSource).toMatch(/initViewport/);
    expect(appSource).not.toMatch(/ajax-dashboard-open/);
    expect(appViewportSource).toMatch(/--app-band-top:\s*var\(--app-top/);
    expect(appViewportSource).toMatch(/--app-band-height:\s*var\(--app-height/);
    expect(appSource).not.toMatch(/--app-height|--app-top/);
  });

  it("pins app-viewport to the keyboard band when html.keyboard-open", () => {
    expect(appViewportSource).toMatch(/:global\(html\.keyboard-open\)\s+\.app-viewport\s*\{/);
    expect(appViewportSource).toMatch(
      /:global\(html\.keyboard-open\)\s+\.app-viewport\s*\{[^}]*position:\s*fixed/,
    );
    expect(appViewportSource).toMatch(
      /:global\(html\.keyboard-open\)\s+\.app-viewport\s*\{[^}]*top:\s*var\(--app-band-top/,
    );
    expect(appViewportSource).toMatch(
      /:global\(html\.keyboard-open\)\s+\.app-viewport\s*\{[^}]*height:\s*var\(--app-band-height/,
    );
  });

  it("hides chrome and clears task route-scroll padding when keyboard-open", () => {
    const stylesSource = loadStylesSource();
    const mobileBlocks = [...stylesSource.matchAll(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n\}/g,
    )];
    const mobileCss = mobileBlocks.map((match) => match[1]).join("\n");

    expect(mobileCss).toMatch(
      /html\.keyboard-open \.bottom-nav[\s\S]*?\{[^}]*display:\s*none/,
    );
    expect(mobileCss).toMatch(
      /html\.keyboard-open \.cockpit-chrome[\s\S]*?\{[^}]*display:\s*none/,
    );
    expect(mobileCss).toMatch(
      /html\.keyboard-open \[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s*\{[^}]*padding-bottom:\s*0/,
    );
  });

  it("hides route-scroll scrollbar chrome so content keeps full width", () => {
    const stylesSource = loadStylesSource();
    const routeScrollRule = stylesSource.match(
      /\[data-testid="route-scroll"\]\s*\{([^}]*)\}/,
    )?.[1] ?? "";

    expect(routeScrollRule).toMatch(/scrollbar-width:\s*none/);
    expect(routeScrollRule).toMatch(/-ms-overflow-style:\s*none/);
    expect(stylesSource).toMatch(
      /\[data-testid="route-scroll"\]::-webkit-scrollbar\s*\{[^}]*(?:display:\s*none|width:\s*0)/,
    );
  });

  it("sets the document title per route", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(App);
    expect(document.title).toBe("Ajax");

    setHash("#/settings");
    await tick();
    expect(document.title).toBe("Settings — Ajax");

    setHash("#/t/web%2Ffix-login");
    await tick();
    expect(document.title).toBe("web/fix-login — Ajax");
  });

  it("marks the dashboard nav button as current", async () => {
    const { container } = render(App);
    const dashboardNav = () =>
      container.querySelector<HTMLButtonElement>("[data-bottom-route='#/']")!;

    expect(dashboardNav()).toHaveAttribute("aria-current", "page");

    setHash("#/settings");
    await tick();
    expect(dashboardNav()).not.toHaveAttribute("aria-current");
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

  it("resumes the task once when its route is entered, and re-resumes a different handle", async () => {
    const operations: Array<{ task_handle: string; action: string }> = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body)));
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { findByTestId } = render(App);

    // Dashboard route must never resume.
    await tick();
    expect(operations).toHaveLength(0);

    setHash("#/t/web%2Ffix-login");
    await findByTestId("outlet-task");
    await vi.waitFor(() =>
      expect(operations).toEqual([{ task_handle: "web/fix-login", action: "resume", request_id: expect.any(String) }]),
    );

    // Leaving and re-entering a different handle is a fresh open → a fresh resume.
    setHash("#/");
    setHash("#/t/web%2Fother");
    await vi.waitFor(() => expect(operations).toHaveLength(2));
    expect(operations[1]).toMatchObject({ task_handle: "web/other", action: "resume" });

    // Let the in-flight detail load land while WebSocket is still stubbed —
    // a late mount after unstubAllGlobals hits jsdom's real WebSocket and
    // rejects asynchronously outside any test.
    await findByTestId("task-terminal-panel");
  });

  it("ignores a stale detail response after switching tasks", async () => {
    let resolveFirstDetail!: (value: unknown) => void;
    const firstDetail = new Promise((resolve) => (resolveFirstDetail = resolve));
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return firstDetail;
        if (path === "/api/tasks/web%2Fother")
          return Promise.resolve(
            jsonResponse({ ...taskDetail, qualified_handle: "web/other", title: "Other task" }),
          );
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { findByText, queryByText } = render(App);
    setHash("#/t/web%2Ffix-login");
    await tick();
    setHash("#/t/web%2Fother");
    await findByText("Other task");

    // The slow response for the task we left must not clobber the open one.
    resolveFirstDetail(jsonResponse({ ...taskDetail, title: "STALE fix-login" }));
    // Macrotask boundary: let the whole fetch→parse→assign chain settle.
    await new Promise((resolve) => setTimeout(resolve, 0));
    await tick();
    expect(queryByText("STALE fix-login")).not.toBeInTheDocument();
    expect(queryByText("Other task")).toBeInTheDocument();
  });

  it("mounts the task terminal panel after detail loads", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
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

  it("surfaces an update banner when the API version changes", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const { container } = render(App);
    const banner = container.querySelector(".update-banner") as HTMLButtonElement;

    expect(banner.hidden).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    expect(banner.hidden).toBe(true);

    await vi.advanceTimersByTimeAsync(30000);

    await vi.waitFor(() => expect(banner.hidden).toBe(false));
    expect(banner).toHaveTextContent("Update ready — tap to reload");
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
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
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
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
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

    const { findByText, queryByText } = render(App);
    setHash("#/t/web%2Ffix-login");
    expect(await findByText("disconnected: HTTP 500")).toBeInTheDocument();

    // Flush the dashboard intermediate so the detail effect observes handle=null
    // before reopening the same task (sync double-hashchange would otherwise batch).
    setHash("#/");
    await tick();
    setHash("#/t/web%2Ffix-login");

    expect(await findByText("connected")).toBeInTheDocument();
    expect(queryByText("disconnected: HTTP 500")).toBeNull();
  });
});
