// Shared mock harness for the Web Cockpit e2e suites. API responses are mocked
// via addInitScript (overrides globalThis.fetch before the app boots) so tests
// run without a live Rust server. Extracted from smoke.test.ts so every e2e
// file drives the app through one fixture set.

import { expect, type Page } from "@playwright/test";

// ---- fixture data --------------------------------------------------------

export const COCKPIT_FIXTURE = {
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

export const DETAIL_FIXTURE = {
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

export const VERSION_A = { version: "0.20.5" };
export const VERSION_B = { version: "0.21.0-new" };

// ---- fetch mock helper ---------------------------------------------------

export async function mockFetch(page: Page, extra: Record<string, unknown> = {}) {
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

export async function mockTerminalWebSocket(page: Page) {
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

export const terminalFrames = (page: Page) =>
  page.evaluate(() => (window as unknown as { __terminalFrames: unknown[] }).__terminalFrames);

export const terminalPanel = (page: Page) =>
  page.locator("[data-testid='task-terminal-panel'][data-terminal-engine='ghostty']");

export const terminalToolbar = (page: Page) =>
  page.locator("[data-testid='terminal-bottom-controls']").getByRole("toolbar", {
    name: "Terminal keys",
  });

export async function waitForTerminalSocket(page: Page) {
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
