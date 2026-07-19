import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, waitFor, screen, act, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import App from "./App";
import appSource from "./App.tsx?raw";
import appViewportSource from "./AppViewport.tsx?raw";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { taskHash } from "@/shared/lib/routes";

function taskTerminalStylesSection(stylesSource: string): string {
  const start = stylesSource.indexOf("/* TaskTerminal");
  const end = stylesSource.indexOf("/* TAILWIND THEME");
  if (start < 0 || end <= start) return "";
  return stylesSource.slice(start, end);
}

function taskTerminalMobileBlock(stylesSource: string): string {
  const tail = taskTerminalStylesSection(stylesSource);
  const match = tail.match(
    /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*)\n\}\s*$/,
  );
  return match?.[1] ?? "";
}

function loadStylesSource(): string {
  const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
  return readFileSync(join(testDir, "../styles.css"), "utf8");
}

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
    const { container } = render(<App />);
    expect(screen.getByRole("heading", { name: "Ajax" })).toBeInTheDocument();
    expect(container.querySelector(".connection-status")).toBeInTheDocument();
    expect(container.querySelector(".update-banner")).toBeInTheDocument();
    expect(container.querySelector(".bottom-nav")).toBeInTheDocument();
    expect(container.querySelector("[data-bottom-action='new-task']")).toBeInTheDocument();
    expect(screen.getByTestId("app-main")).toBeInTheDocument();
    expect(screen.getByTestId("route-scroll")).toBeInTheDocument();
  });

  it("live-dot pulses only when connected", () => {
    expect(appSource).toMatch(
      /is-live[\s\S]*connection === "connected"|connection === "connected"[\s\S]*is-live/,
    );
    const stylesSource = loadStylesSource();
    expect(stylesSource).toMatch(
      /\.live-dot\s*\{[^}]*background:\s*var\(--ink-faint\)/,
    );
    expect(stylesSource).toMatch(
      /\.live-dot\.is-live\s*\{[^}]*animation:\s*pulse/,
    );
  });

  it("syncs --app-height from the visual viewport on mount", () => {
    vi.stubGlobal("visualViewport", {
      height: 712,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    document.documentElement.style.removeProperty("--app-height");
    render(<App />);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("712px");
  });

  it("shows the dashboard outlet by default", () => {
    const { container } = render(<App />);
    expect(container.querySelector("[data-outlet='dashboard']")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='settings']")).toBeNull();
  });

  it("exposes layout primitives for viewport and scroll ownership", () => {
    const stylesSource = loadStylesSource();
    render(<App />);
    expect(screen.getByTestId("app-viewport")).toBeInTheDocument();
    expect(screen.getByTestId("app-shell")).toBeInTheDocument();
    expect(screen.getByTestId("app-main")).toBeInTheDocument();
    expect(screen.getByTestId("route-scroll")).toBeInTheDocument();
    expect(appSource).not.toMatch(/initViewport/);
    expect(appViewportSource).toMatch(/initViewport/);
    expect(appSource).not.toMatch(/ajax-dashboard-open/);
    expect(stylesSource).toMatch(/--app-band-top:\s*var\(--app-top/);
    expect(stylesSource).toMatch(/--app-band-height:\s*var\(--app-height/);
    expect(appSource).not.toMatch(/--app-height|--app-top/);
  });

  it("pins app-viewport to the keyboard band when html.keyboard-open", () => {
    const stylesSource = loadStylesSource();
    const keyboardRule =
      stylesSource.match(/html\.keyboard-open\s+\.app-viewport\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(keyboardRule).toMatch(/position:\s*fixed/);
    expect(keyboardRule).toMatch(/top:\s*var\(--app-top,\s*var\(--app-band-top,\s*0px\)\)/);
    expect(keyboardRule).toMatch(
      /height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/,
    );
    expect(keyboardRule).toMatch(
      /max-height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/,
    );
    expect(keyboardRule).not.toMatch(/bottom:\s*max/);
    expect(keyboardRule).not.toMatch(/bottom:\s*calc/);
  });

  it("zeros horizontal padding on the mobile task route-scroll", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    expect(mobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s*\{[^}]*padding-left:\s*0/,
    );
    expect(mobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s*\{[^}]*padding-right:\s*0/,
    );
  });

  it("mobile task route keeps outlet flex without growing the closed-keyboard terminal panel", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    expect(mobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s*\{[^}]*display:\s*flex/,
    );
    expect(mobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s*>\s*\[data-outlet="task"\]\s*\{[^}]*flex:\s*1\s+1\s+0%/,
    );
    expect(mobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s+\.task-detail\s*\{[^}]*flex:\s*1\s+1\s+0%/,
    );
    // Closed-keyboard: do not flex-grow the terminal panel (causes tall empty PTY rows).
    expect(mobileBlock).not.toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s+\.terminal-panel:not\(\.is-expanded\)\s*\{[^}]*flex:\s*1\s+1\s+0%/,
    );
    // Keyboard-open still flex-fills the panel under the fixed task-detail band.
    expect(mobileBlock).toMatch(
      /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s+\.terminal-panel:not\(\.is-expanded\)\s*\{[^}]*flex:\s*1\s+1\s+0%/,
    );
  });

  it("keyboard-open keeps task header and interact panel visible", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    // Header/status stay visible under keyboard-open (flex:none), and must not
    // share a display:none rule with meta-details the way a loose regex can misread.
    expect(mobileBlock).toMatch(
      /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s+\.detail-header,\s*html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s+\.interact-panel\s*\{[^}]*flex:\s*none/,
    );
    expect(mobileBlock).not.toMatch(
      /html\.keyboard-open[^{]*\.task-detail\s+\.detail-header[^{]*\{[^}]*display:\s*none/,
    );
    expect(mobileBlock).not.toMatch(
      /html\.keyboard-open[^{]*\.task-detail\s+\.interact-panel[^{]*\{[^}]*display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header[\s\S]*?display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.interact-panel[\s\S]*?display:\s*none/,
    );
  });

  it("keyboard-open still hides bottom nav and cockpit chrome", () => {
    const stylesSource = loadStylesSource();

    expect(stylesSource).toMatch(
      /html\.keyboard-open\s+\.cockpit-chrome[\s\S]*?display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.keyboard-open\s+\.bottom-nav[\s\S]*?display:\s*none/,
    );
  });

  it("expanded terminal panel matches fullscreen band without safe-area top padding", () => {
    const stylesSource = loadStylesSource();
    const expandedRule =
      taskTerminalStylesSection(stylesSource).match(
        /html\.terminal-expanded\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n  \}/,
      )?.[1] ?? "";

    expect(expandedRule).toMatch(/top:\s*var\(--app-top/);
    expect(expandedRule).toMatch(
      /height:\s*var\(--app-height,\s*var\(--app-band-height/,
    );
    expect(expandedRule).not.toMatch(/bottom:\s*max/);
    expect(expandedRule).toMatch(/overflow:\s*hidden/);
    expect(expandedRule).not.toMatch(/padding:\s*env\(safe-area-inset-top\)/);
  });

  it("keyboard-open non-expanded terminal fills remaining band", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock = taskTerminalMobileBlock(stylesSource);

    const keyboardWrapRule =
      mobileBlock.match(
        /html\.keyboard-open\s+\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(keyboardWrapRule).toMatch(/flex:\s*1\s+1\s+0%/);
    expect(keyboardWrapRule).toMatch(/min-height:\s*0/);
    expect(keyboardWrapRule).not.toMatch(/height:\s*min\(38vh/);
  });

  it("keyboard-open pins task detail to the app band so hotkeys sit above the keyboard", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    const taskDetailRule =
      mobileBlock.match(
        /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(taskDetailRule).toMatch(/position:\s*fixed/);
    expect(taskDetailRule).toMatch(/top:\s*var\(--app-top,\s*var\(--app-band-top,\s*0px\)\)/);
    expect(taskDetailRule).toMatch(
      /height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/,
    );
    expect(taskDetailRule).toMatch(
      /max-height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/,
    );
    expect(taskDetailRule).not.toMatch(/bottom:\s*max/);
    expect(taskDetailRule).not.toMatch(/bottom:\s*calc/);
  });

  it("does not pin task-detail under keyboard-open while terminal is expanded", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    // Expanded panel owns the band; a fixed overflow parent would become the
    // containing block on iOS and push the fullscreen terminal under the keyboard.
    expect(mobileBlock).toMatch(
      /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s*\{/,
    );
    expect(mobileBlock).not.toMatch(
      /html\.keyboard-open\s+\.task-detail\s*\{[^}]*position:\s*fixed/,
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

    render(<App />);
    expect(document.title).toBe("Ajax");

    setHash("#/settings");
    await waitFor(() => expect(document.title).toBe("Settings — Ajax"));

    setHash("#/t/web%2Ffix-login");
    await waitFor(() => expect(document.title).toBe("web/fix-login — Ajax"));
  });

  it("marks the dashboard nav button as current", async () => {
    const { container } = render(<App />);
    const dashboardNav = () =>
      container.querySelector<HTMLButtonElement>("[data-bottom-route='#/']")!;

    expect(dashboardNav()).toHaveAttribute("aria-current", "page");

    setHash("#/settings");
    await waitFor(() => expect(dashboardNav()).not.toHaveAttribute("aria-current"));
  });

  it("styles the current bottom-nav page with an accent selected state", () => {
    const stylesSource = loadStylesSource();
    const currentPageRule =
      stylesSource.match(/\.bottom-nav button\[aria-current(?:="page")?\]\s*\{([^}]*)\}/)?.[1] ??
      "";

    expect(stylesSource).toMatch(/\.bottom-nav button\[aria-current/);
    expect(currentPageRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
  });

  it("shows a dashboard skeleton while the cockpit projection is loading", () => {
    const { container } = render(<App />);
    expect(screen.getByTestId("dashboard-skeleton")).toBeInTheDocument();
    expect(container.querySelector(".empty")).toBeNull();
  });

  it("shows a task skeleton while a task detail is loading", async () => {
    render(<App />);
    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");
    expect(screen.getByTestId("task-skeleton")).toBeInTheDocument();
  });

  it("shows the settings outlet on the settings route", async () => {
    const { container } = render(<App />);
    setHash("#/settings");
    expect(await screen.findByTestId("outlet-settings")).toBeInTheDocument();
    expect(container.querySelector("[data-outlet='dashboard']")).toBeNull();
  });

  it("shows the task outlet on a task route", async () => {
    render(<App />);
    setHash("#/t/web%2Ffix-login");
    expect(await screen.findByTestId("outlet-task")).toBeInTheDocument();
  });

  it("renders task detail while the resume operation is still in flight", async () => {
    let releaseResume!: (value: ReturnType<typeof jsonResponse>) => void;
    const resumePending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      releaseResume = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return resumePending;
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");

    releaseResume(jsonResponse({ ok: true }));
    await waitFor(() => expect(true).toBe(true));
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

    render(<App />);

    // Dashboard route must never resume.
    await waitFor(() => expect(operations).toHaveLength(0));

    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");
    await vi.waitFor(() =>
      expect(operations).toEqual([{ task_handle: "web/fix-login", action: "resume", request_id: expect.any(String) }]),
    );

    // Leaving and re-entering a different handle is a fresh open → a fresh resume.
    setHash("#/");
    setHash("#/t/web%2Fother");
    await vi.waitFor(() => expect(operations).toHaveLength(2));
    expect(operations[1]).toMatchObject({ task_handle: "web/other", action: "resume" });

    await screen.findByTestId("outlet-task");
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

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    await waitFor(() => expect(true).toBe(true));
    setHash("#/t/web%2Fother");
    await screen.findByText("Other task");

    // The slow response for the task we left must not clobber the open one.
    resolveFirstDetail(jsonResponse({ ...taskDetail, title: "STALE fix-login" }));
    // Macrotask boundary: let the whole fetch→parse→assign chain settle.
    await new Promise((resolve) => setTimeout(resolve, 0));
    await waitFor(() => expect(true).toBe(true));
    expect(screen.queryByText("STALE fix-login")).not.toBeInTheDocument();
    expect(screen.getByText("Other task")).toBeInTheDocument();
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

    const { container } = render(<App />);
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

    // Dashboard cadence is 1000ms: three ticks add three polls.
    await vi.advanceTimersByTimeAsync(3000);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(4));
  });

  it("reschedules the cockpit interval when the route cadence changes", async () => {
    vi.useFakeTimers();
    const { fetchMock, cockpitCalls } = cockpitCountingFetch();
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(1));

    // Task route slows the cadence to 5000ms. If the old 1000ms interval were
    // left running, 4000ms would add four polls instead of none.
    await act(async () => {
      setHash(taskHash("web/a"));
    });
    // Guard: a wrong prefix would silently leave the route on dashboard and the
    // 1000ms cadence would look correct.
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
    const afterRouteChange = cockpitCalls();

    await vi.advanceTimersByTimeAsync(4000);
    expect(cockpitCalls()).toBe(afterRouteChange);

    await vi.advanceTimersByTimeAsync(1000);
    await vi.waitFor(() => expect(cockpitCalls()).toBe(afterRouteChange + 1));
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

    // A focus resume triggers exactly one extra cockpit load, not one per
    // re-render that has happened since mount.
    const beforeFocus = cockpitCalls();
    window.dispatchEvent(new Event("focus"));
    await vi.waitFor(() => expect(cockpitCalls()).toBe(beforeFocus + 1));
    await vi.advanceTimersByTimeAsync(0);
    expect(cockpitCalls()).toBe(beforeFocus + 1);
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

    // Task-route cadence is 5000ms; drive three polls, each with a changed payload.
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
    within(screen.getByTestId("task-load-error")).getByRole("button", { name: "Retry" }).click();
    await screen.findByText("Fix login");
    expect(detailCalls).toBe(callsBeforeRetry + 1);
  });
});
