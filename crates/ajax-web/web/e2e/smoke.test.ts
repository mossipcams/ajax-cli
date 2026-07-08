// Operator-flow smoke suite. API responses are mocked via addInitScript
// (overrides globalThis.fetch before the app boots) so these tests run
// without a live Rust server. They verify Svelte routing, rendering,
// polling, confirmation, and connection-recovery flows in a real browser.

import { test, expect, type Page } from "@playwright/test";

// ---- fixture data --------------------------------------------------------

const COCKPIT_FIXTURE = {
  backend: { authority: "host-native", control_enabled: true, warning: null },
  repos: { repos: [{ name: "web" }, { name: "api" }] },
  cards: [
    {
      id: "web/fix-login",
      qualified_handle: "web/fix-login",
      repo: "web",
      title: "Fix login",
      status: "waiting",
      status_explanation: "Waiting for review",
      actions: [
        { action: "review", label: "Review", destructive: false, confirmation_required: false },
        { action: "drop",   label: "Drop",   destructive: true,  confirmation_required: true  },
      ],
    },
    {
      id: "api/add-auth",
      qualified_handle: "api/add-auth",
      repo: "api",
      title: "Add auth",
      status: "running",
      status_explanation: null,
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/fix-login", severity: 2 }] },
};

const DETAIL_FIXTURE = {
  qualified_handle: "web/fix-login",
  repo: "web",
  title: "Fix login",
  branch: "ajax/fix-login",
  base_branch: "main",
  worktree_path: "/repo/web/ajax-fix-login",
  tmux_session: "ajax-web-fix-login",
  lifecycle: "reviewable",
  agent: "codex",
  agent_status: "idle",
  status: "waiting",
  status_explanation: "Waiting for review",
  runtime_observation_error: null,
  actions: [
    { action: "review", label: "Review", destructive: false, confirmation_required: false },
    { action: "drop",   label: "Drop",   destructive: true,  confirmation_required: true  },
  ],
  live_status_kind: null,
  live_status_summary: null,
  agent_activity: null,
  git: { unpushed_commits: 1 },
  tmux: null,
  annotations: [],
  created_unix_secs: 1700000000,
  last_activity_unix_secs: 1700001000,
  agent_attempts: [],
};

const VERSION_A = { version: "0.20.5" };
const VERSION_B = { version: "0.21.0-new" };

// ---- fetch mock helper ---------------------------------------------------

async function mockFetch(page: Page, extra: Record<string, unknown> = {}) {
  const routes: Record<string, unknown> = {
    "/api/cockpit":    COCKPIT_FIXTURE,
    "/api/version":    VERSION_A,
    "/api/health":     { status: "ok" },
    "/api/operations": { cockpit: COCKPIT_FIXTURE, output: "ok", error: null },
    "/api/server/restart": {},
    "__detail__":      DETAIL_FIXTURE,
    ...extra,
  };

  await page.addInitScript((routeMap: Record<string, unknown>) => {
    const real = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;

      if (Object.prototype.hasOwnProperty.call(routeMap, path)) {
        return new Response(JSON.stringify(routeMap[path]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (/^\/api\/tasks\/[^/]+\/pane$/.test(path)) {
        return new Response(
          JSON.stringify({ sequence: 0, lines: [], tmux_exists: false, state: null }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (/^\/api\/tasks\/[^/]+$/.test(path)) {
        return new Response(JSON.stringify(routeMap["__detail__"]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (path.startsWith("/api/")) {
        return new Response(JSON.stringify({ error: "not found" }), {
          status: 404,
          headers: { "content-type": "application/json" },
        });
      }
      return real(input, init);
    };
  }, routes);
}

async function mockTerminalWebSocket(page: Page) {
  await page.addInitScript(() => {
    const sockets: unknown[] = [];
    const frames: unknown[] = [];

    class MockTerminalWebSocket {
      static CONNECTING = 0;
      static OPEN = 1;
      static CLOSING = 2;
      static CLOSED = 3;
      readyState = MockTerminalWebSocket.CONNECTING;
      readonly url: string;
      private listeners: Record<string, Array<(event: Event) => void>> = {};

      constructor(url: string) {
        this.url = url;
        sockets.push(this);
        setTimeout(() => {
          if (this.readyState !== MockTerminalWebSocket.CONNECTING) return;
          this.readyState = MockTerminalWebSocket.OPEN;
          this.dispatch("open", new Event("open"));
        }, 0);
      }

      addEventListener(type: string, handler: (event: Event) => void) {
        (this.listeners[type] ??= []).push(handler);
      }

      removeEventListener(type: string, handler: (event: Event) => void) {
        this.listeners[type] = (this.listeners[type] ?? []).filter((item) => item !== handler);
      }

      send(data: string | ArrayBuffer | ArrayBufferView) {
        if (typeof data === "string") {
          frames.push(JSON.parse(data));
          return;
        }
        const bytes = ArrayBuffer.isView(data)
          ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
          : new Uint8Array(data);
        frames.push({ type: "input", data: new TextDecoder().decode(bytes) });
      }

      close() {
        this.emitClose();
      }

      emitMessage(data: string) {
        this.dispatch("message", new MessageEvent("message", { data }));
      }

      emitClose() {
        this.readyState = MockTerminalWebSocket.CLOSED;
        this.dispatch("close", new CloseEvent("close"));
      }

      private dispatch(type: string, event: Event) {
        for (const handler of this.listeners[type] ?? []) handler(event);
      }
    }

    Object.defineProperty(window, "__terminalSockets", {
      value: sockets,
      configurable: true,
    });
    Object.defineProperty(window, "__terminalFrames", {
      value: frames,
      configurable: true,
    });
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: async () => "echo pasted" },
      configurable: true,
    });
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = MockTerminalWebSocket;
  });
}

const terminalFrames = (page: Page) =>
  page.evaluate(() => (window as unknown as { __terminalFrames: unknown[] }).__terminalFrames);

const terminalPanel = (page: Page) =>
  page.locator("[data-testid='task-terminal-panel'][data-terminal-engine='ghostty']");

const terminalToolbar = (page: Page) =>
  page.locator("[data-testid='terminal-bottom-controls']").getByRole("toolbar", {
    name: "Terminal keys",
  });

async function waitForTerminalSocket(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { __terminalSockets: Array<{ readyState: number }> })
            .__terminalSockets?.[0]?.readyState,
      ),
    )
    .toBe(1);
}

// ---- tests ---------------------------------------------------------------

// Task list rows show `qualified_handle`, not `title`. Inbox cards also show
// `status_explanation`. Use handles as stable selectors.

test("dashboard renders tasks from cockpit fixture", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");

  // Inbox card shows the handle and status_explanation
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  // Calm group shows api/add-auth handle in a task row
  await expect(page.getByText("api/add-auth")).toBeVisible();
});

test("project filter shows only matching repo tasks", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Click the "web" project pill
  await page.locator("button.project-pill").filter({ hasText: "web" }).first().click();

  await expect(page.getByText("web/fix-login")).toBeVisible();
  await expect(page.getByText("api/add-auth")).not.toBeVisible();
});

test("task detail renders server status and actions", async ({ page }, testInfo) => {
  await mockFetch(page);
  // Use correct task hash prefix from routes.ts: #/t/
  await page.goto("/app.html#/t/web%2Ffix-login");

  if (testInfo.project.name === "mobile-webkit") {
    await expect(page.locator(".interact-pill")).toContainText("Waiting", { timeout: 10_000 });
  } else {
    await expect(page.getByText("Waiting for review")).toBeVisible({ timeout: 10_000 });
  }
  await expect(page.locator("[data-action='review']")).toBeVisible();
});

test("mobile task terminal opens ghostty and sends toolbar input", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(terminalPanel(page).locator("canvas")).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const toolbar = terminalToolbar(page);
  await toolbar.getByRole("button", { name: "Esc" }).click();
  await toolbar.getByRole("button", { name: "Tab" }).click();
  await toolbar.getByRole("button", { name: "⌃C" }).click();
  await toolbar.getByRole("button", { name: "Ctrl" }).click();
  await toolbar.getByRole("button", { name: "←" }).click();

  await expect
    .poll(() => terminalFrames(page))
    .toEqual(
      expect.arrayContaining([
        { type: "input", data: "\x1b" },
        { type: "input", data: "\t" },
        { type: "input", data: "\x03" },
        { type: "input", data: "\x1b[1;5D" },
      ]),
    );
});

test("mobile task terminal resize and reconnect flows stay wired", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page).locator("canvas")).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.setViewportSize({ width: 844, height: 390 });
  await expect
    .poll(async () => {
      const frames = await terminalFrames(page);
      return frames.filter(
        (frame) => (frame as { type?: string }).type === "resize",
      ).length;
    })
    .toBeGreaterThan(0);

  const closeDebug = await page.evaluate(async () => {
    const socket = (window as unknown as { __terminalSockets: Array<{
      emitClose(): void;
      listeners?: Record<string, Array<unknown>>;
      readyState: number;
    }> }).__terminalSockets.at(-1)!;
    const listenerCounts = Object.fromEntries(
      Object.entries(socket.listeners ?? {}).map(([name, handlers]) => [name, handlers.length]),
    );
    socket.emitClose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    return {
      listenerCounts,
      readyState: socket.readyState,
      socketCount: (window as unknown as { __terminalSockets: unknown[] }).__terminalSockets.length,
      status:
        document.querySelector("[data-testid='terminal-status']")?.textContent?.trim() ?? "",
    };
  });
  expect(closeDebug.status, JSON.stringify(closeDebug)).toContain("Reconnecting");
});

test("new task sheet stays inside the visible band when the keyboard opens", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const titleInput = page.locator("#new-task-title-input");
  await expect(titleInput).toBeVisible();
  await titleInput.click();

  // Simulate viewport.ts reacting to the iOS soft keyboard: the visual
  // viewport shrinks to a 460px band and Safari pans the page down 40px.
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "460px");
    document.documentElement.style.setProperty("--app-top", "40px");
  });

  // The focused input must sit inside the visible band [40, 40 + 460] —
  // otherwise it is hidden behind the keyboard while the user types.
  const box = await titleInput.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(40);
  expect(box!.y + box!.height).toBeLessThanOrEqual(40 + 460);
});

test("non-destructive action completes without a second tap", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='review']")).toBeVisible({ timeout: 10_000 });

  await page.locator("[data-action='review']").click();

  // Operation mock returns the refreshed cockpit; task outlet stays visible
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("destructive action requires two taps to execute", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='drop']")).toBeVisible({ timeout: 10_000 });

  // First tap: enters confirming state
  await page.locator("[data-action='drop']").click();
  await expect(page.locator(".action.confirming")).toBeVisible({ timeout: 3_000 });

  // Second tap: executes
  await page.locator(".action.confirming").click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("connection error shows backend unreachable state", async ({ page }) => {
  // Override to throw on cockpit — other routes still work
  await page.addInitScript(() => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      if (url.includes("/api/cockpit")) throw new TypeError("Failed to fetch");
      return orig(input, init);
    };
  });

  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", { timeout: 8_000 });

  // Tap Retry — cockpit is still failing, so still unreachable
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.locator(".connection-status")).toContainText("unreachable");
});

test("settings view renders restart and diagnostics controls", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");

  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByRole("button", { name: /Restart/i })).toBeVisible();
  await expect(
    page.locator("[data-testid='outlet-settings']").getByRole("button", { name: /Diagnostics/i }).first()
  ).toBeVisible();
});

test("update banner appears when version changes between polls", async ({ page }) => {
  await page.addInitScript((versions: { a: unknown; b: unknown; cockpit: unknown }) => {
    let count = 0;
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/cockpit")
        return new Response(
          JSON.stringify(versions.cockpit),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      if (path === "/api/version") {
        count++;
        return new Response(
          JSON.stringify(count === 1 ? versions.a : versions.b),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (path.startsWith("/api/")) {
        return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
      }
      return orig(input, init);
    };
  }, { a: VERSION_A, b: VERSION_B, cockpit: COCKPIT_FIXTURE });

  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Trigger a second version check via focus event
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));

  await expect(page.locator(".update-banner")).not.toHaveAttribute("hidden", { timeout: 10_000 });
});
