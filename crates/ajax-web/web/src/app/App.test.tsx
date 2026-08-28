import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, waitFor } from "@testing-library/react";
import { join } from "node:path";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import App from "./App";
import appSource from "./App.tsx?raw";
import appViewportSource from "./AppViewport.tsx?raw";
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
  return readOrderedStylesSource(join(testDir, ".."));
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
    // Tests that fake a hidden document redefine these; unstubAllGlobals does
    // not undo defineProperty, so reset them here.
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

  it("renders the shared chrome", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Ajax" })).toBeInTheDocument();
    expect(screen.getByTestId("connection-status")).toBeInTheDocument();
    expect(screen.getByTestId("update-banner")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Mobile navigation" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Control" })).not.toBeInTheDocument();
    expect(screen.getByTestId("app-main")).toBeInTheDocument();
    expect(screen.getByTestId("route-scroll")).toBeInTheDocument();
  });

  it("live-dot uses accent when connected without infinite pulse", () => {
    expect(appSource).toMatch(
      /is-live[\s\S]*connection === "connected"|connection === "connected"[\s\S]*is-live/,
    );
    const stylesSource = loadStylesSource();
    expect(stylesSource).toMatch(
      /\.live-dot\s*\{[^}]*background:\s*var\(--ink-faint\)/,
    );
    expect(stylesSource).toMatch(
      /\.live-dot\.is-live\s*\{[^}]*background:\s*var\(--accent\)/,
    );
    expect(stylesSource).not.toMatch(
      /\.live-dot\.is-live\s*\{[^}]*animation:[^}]*pulse[^}]*infinite/,
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
    render(<App />);
    expect(screen.getByTestId("outlet-dashboard")).toBeInTheDocument();
    expect(screen.queryByTestId("outlet-settings")).not.toBeInTheDocument();
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
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s+\[data-outlet="task"\]\s*\{[^}]*flex:\s*1\s+1\s+0%/,
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
    expect(stylesSource).not.toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s*\{[^}]*display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header[\s\S]*?pointer-events:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s+\.detail-header-controls[\s\S]*?pointer-events:\s*auto/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s+\.back[\s\S]*?display:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.detail-header\s+\.session-head-details\s*\{([^}]*)\}/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded\s+\.task-detail\s+\.interact-panel[\s\S]*?display:\s*none/,
    );
  });

  it("keyboard-open inline task-detail pads safe-area top so the whole header row clears the notch", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    const taskDetailRule =
      mobileBlock.match(
        /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    // Cockpit chrome (owner of safe-area top) is hidden while keyboard-open; the
    // fixed task page must carry that inset so back + title + status stay usable.
    expect(taskDetailRule).toMatch(/padding-top:\s*env\(safe-area-inset-top\)/);
  });

  it("mobile task detail-header sticks so the whole chrome row stays on-screen while scrolling", () => {
    const stylesSource = loadStylesSource();
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";

    const stickyRule =
      mobileBlock.match(
        /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\)\s+\.task-detail\s+\.detail-header\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(stickyRule).toMatch(/position:\s*sticky/);
    expect(stickyRule).toMatch(/top:\s*0/);
    expect(stickyRule).toMatch(/z-index:\s*[1-9]\d*/);
    expect(stickyRule).toMatch(/background:\s*var\(--paper\)/);
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
        /html\.terminal-expanded\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n {2}\}/,
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

  it("cross-slide wrappers use display:contents at rest so the mobile flex chain stays intact", () => {
    const stylesSource = loadStylesSource();

    expect(stylesSource).toMatch(
      /\.page-cross-slide-host\.page-cross-slide-host-idle\s*\{[^}]*display:\s*contents/,
    );
    expect(stylesSource).toMatch(
      /\.page-cross-slide-pane\.page-cross-slide-pane-idle\s*\{[^}]*display:\s*contents/,
    );
    expect(stylesSource).toMatch(
      /\.page-cross-slide-leaving\s*\{[^}]*will-change:\s*transform/,
    );
    expect(stylesSource).toMatch(
      /\.page-cross-slide-entering\s*\{[^}]*will-change:\s*transform/,
    );
    expect(stylesSource).not.toMatch(/\.page-cross-slide-pane\s*\{[^}]*will-change:\s*transform/);
    expect(appSource).toMatch(/page-cross-slide-host-idle/);
    expect(appSource).toMatch(/page-cross-slide-pane-idle/);
  });

  it("hides the xterm DOM scrollbar the viewport override cannot reach", () => {
    const stylesSource = loadStylesSource();

    expect(stylesSource).toMatch(
      /\.terminal-host\s+\.xterm-scrollable-element\s*>\s*\.scrollbar\s*\{[^}]*display:\s*none\s*!important/,
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
    render(<App />);
    const dashboardNav = () => screen.getByRole("button", { name: "Dashboard" });

    expect(dashboardNav()).toHaveAttribute("aria-current", "page");

    setHash("#/settings");
    await waitFor(() => expect(dashboardNav()).not.toHaveAttribute("aria-current"));
  });

  it("uses opaque paper on cockpit-chrome and bottom-nav without backdrop-filter (#836 black box)", () => {
    const stylesSource = loadStylesSource();
    const cockpitChromeRule =
      stylesSource.match(/(?:^|\n)\.cockpit-chrome\s*\{([^}]*)\}/)?.[1] ?? "";
    const bottomNavRule =
      stylesSource.match(/(?:^|\n)\.bottom-nav\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(cockpitChromeRule).not.toMatch(/(?:-webkit-)?backdrop-filter\s*:/);
    expect(bottomNavRule).not.toMatch(/(?:-webkit-)?backdrop-filter\s*:/);
    expect(cockpitChromeRule).toMatch(/background:\s*var\(--paper\)/);
    expect(bottomNavRule).toMatch(/background:\s*var\(--paper\)/);
    expect(bottomNavRule).toMatch(/grid-template-columns:\s*repeat\(2,\s*1fr\)/);
    expect(cockpitChromeRule).not.toMatch(/color-mix/);
    expect(bottomNavRule).not.toMatch(/color-mix/);
  });

  it("styles the current bottom-nav page with an accent selected state", () => {
    const stylesSource = loadStylesSource();
    const currentPageRule =
      stylesSource.match(/\.bottom-nav button\[aria-current(?:="page")?\]\s*\{([^}]*)\}/)?.[1] ??
      "";

    expect(stylesSource).toMatch(/\.bottom-nav button\[aria-current/);
    expect(currentPageRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
  });

  it("anchors the result toast above the bottom nav with full-width actions", () => {
    const stylesSource = loadStylesSource();
    const panelRule = stylesSource.match(/\.result-panel\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(panelRule).toMatch(/bottom:\s*calc\(72px\s*\+\s*12px\)/);
    expect(panelRule).toMatch(/left:\s*50%/);
    expect(panelRule).toMatch(/transform:\s*translateX\(-50%\)/);
    expect(panelRule).not.toMatch(/top:\s*calc\(env\(safe-area-inset-top\)/);
    expect(stylesSource).toMatch(/\.result-actions\s*\{/);
    expect(stylesSource).toMatch(/\.result-actions\s+\.pill[\s\S]*?min-height:\s*44px/);
  });

  it("shows the settings outlet on the settings route", async () => {
    render(<App />);
    setHash("#/settings");
    expect(await screen.findByTestId("outlet-settings")).toBeInTheDocument();
    expect(screen.queryByTestId("outlet-dashboard")).not.toBeInTheDocument();
  });

  it("shows the task outlet on a task route", async () => {
    render(<App />);
    setHash("#/t/web%2Ffix-login");
    expect(await screen.findByTestId("outlet-task")).toBeInTheDocument();
  });

  it("applies swipe enter-left when opening a task from the list", async () => {
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

    sessionStorage.clear();
    render(<App />);
    fireEvent.click(await screen.findByText("Fix login"));

    const outlet = await screen.findByTestId("outlet-task");
    expect(outlet).toHaveClass("ajax-swipe-enter-left");
  });

  it("does not apply swipe enter when opening settings from the header", async () => {
    sessionStorage.clear();
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));

    const outlet = await screen.findByTestId("outlet-settings");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-left");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-right");
  });

  it("renders task detail while the resume operation is still in flight", async () => {
    let releaseResume!: (value: ReturnType<typeof jsonResponse>) => void;
    const resumePending = new Promise<ReturnType<typeof jsonResponse>>((resolve) => {
      releaseResume = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, _init?: RequestInit) => {
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
});
