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

export async function mockTerminalWebSocket(
  page: Page,
  options: {
    autoOpen?: boolean;
    clipboardText?: string;
    clipboardUnavailable?: boolean;
  } = {},
) {
  const autoOpen = options.autoOpen ?? true;
  const clipboardText = options.clipboardText ?? "echo pasted";
  const clipboardUnavailable = options.clipboardUnavailable ?? false;
  await page.addInitScript(({ shouldAutoOpen, clipboard, noClipboard }) => {
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
        if (shouldAutoOpen) {
          setTimeout(() => {
            if (this.readyState !== MockTerminalWebSocket.CONNECTING) return;
            this.emitOpen();
          }, 0);
        }
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

      emitOpen() {
        if (this.readyState === MockTerminalWebSocket.OPEN) return;
        this.readyState = MockTerminalWebSocket.OPEN;
        this.dispatch("open", new Event("open"));
      }

      emitMessage(data: string | ArrayBuffer | ArrayBufferView | number[]) {
        const payload =
          typeof data === "string"
            ? data
            : Array.isArray(data)
              ? new Uint8Array(data)
              : ArrayBuffer.isView(data)
                ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
                : new Uint8Array(data);
        this.dispatch("message", new MessageEvent("message", { data: payload }));
      }

      emitClose() {
        if (this.readyState === MockTerminalWebSocket.CLOSED) return;
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
      value: noClipboard ? undefined : { readText: async () => clipboard },
      configurable: true,
    });
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = MockTerminalWebSocket;
  }, { shouldAutoOpen: autoOpen, clipboard: clipboardText, noClipboard: clipboardUnavailable });
}

type TerminalMessagePayload = string | number[];

type TerminalSocketHandle = {
  url: string;
  readyState: number;
  emitOpen(): void;
  emitClose(): void;
  emitMessage(data: string | ArrayBuffer | ArrayBufferView | number[]): void;
};

export async function terminalSocketSummaries(
  page: Page,
): Promise<Array<{ url: string; readyState: number }>> {
  return page.evaluate(() => {
    const sockets =
      (window as unknown as { __terminalSockets: TerminalSocketHandle[] }).__terminalSockets ??
      [];
    return sockets
      .filter((socket) => socket.url.includes("/terminal"))
      .map((socket) => ({ url: socket.url, readyState: socket.readyState }));
  });
}

export async function openLatestTerminalSocket(page: Page): Promise<void> {
  await page.evaluate(() => {
    const sockets =
      (window as unknown as { __terminalSockets: TerminalSocketHandle[] }).__terminalSockets ??
      [];
    const socket = sockets.filter((item) => item.url.includes("/terminal")).at(-1);
    if (!socket) throw new Error("no terminal socket");
    socket.emitOpen();
  });
}

export async function closeLatestTerminalSocket(page: Page): Promise<void> {
  await page.evaluate(() => {
    const sockets =
      (window as unknown as { __terminalSockets: TerminalSocketHandle[] }).__terminalSockets ??
      [];
    const socket = sockets.filter((item) => item.url.includes("/terminal")).at(-1);
    if (!socket) throw new Error("no terminal socket");
    socket.emitClose();
  });
}

export async function emitLatestTerminalOutput(
  page: Page,
  chunks: Array<TerminalMessagePayload>,
): Promise<void> {
  await page.evaluate((messageChunks) => {
    const sockets =
      (window as unknown as { __terminalSockets: TerminalSocketHandle[] }).__terminalSockets ??
      [];
    const socket = sockets.filter((item) => item.url.includes("/terminal")).at(-1);
    if (!socket) throw new Error("no terminal socket");
    for (const chunk of messageChunks) {
      socket.emitMessage(chunk);
    }
  }, chunks);
}

export async function failLatestTerminalSocket(
  page: Page,
  message: string,
): Promise<void> {
  await page.evaluate((errorMessage) => {
    const sockets =
      (window as unknown as { __terminalSockets: TerminalSocketHandle[] }).__terminalSockets ??
      [];
    const socket = sockets.filter((item) => item.url.includes("/terminal")).at(-1);
    if (!socket) throw new Error("no terminal socket");
    socket.emitMessage(JSON.stringify({ type: "error", error: errorMessage }));
    socket.emitClose();
  }, message);
}

export const terminalFrames = (page: Page) =>
  page.evaluate(() => (window as unknown as { __terminalFrames: unknown[] }).__terminalFrames);

type TerminalInputFrame = { type: "input"; data: string };

export async function terminalInputFrames(page: Page): Promise<TerminalInputFrame[]> {
  const frames = await terminalFrames(page);
  return frames.filter(
    (frame): frame is TerminalInputFrame =>
      typeof frame === "object" &&
      frame !== null &&
      (frame as { type?: string }).type === "input" &&
      typeof (frame as { data?: unknown }).data === "string",
  );
}

export const terminalSurface = (page: Page) =>
  page.locator("[data-testid='task-terminal-panel']");

export const terminalInteractionSurface = (page: Page) =>
  page.locator("[data-testid='terminal-interaction-surface']");

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

type TerminalResizeFrame = { type: "resize"; cols: number; rows: number };

function isValidPositiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 && Number.isInteger(value);
}

export async function terminalResizeFrames(page: Page): Promise<TerminalResizeFrame[]> {
  const frames = await terminalFrames(page);
  return frames.filter(
    (frame): frame is TerminalResizeFrame =>
      typeof frame === "object" &&
      frame !== null &&
      (frame as { type?: string }).type === "resize" &&
      isValidPositiveInteger((frame as { cols?: unknown }).cols) &&
      isValidPositiveInteger((frame as { rows?: unknown }).rows),
  );
}

export type ViewportEventKind = "resize" | "orientationchange" | "visualViewport.resize";

export async function dispatchViewportEvents(
  page: Page,
  events: ViewportEventKind[],
): Promise<void> {
  await page.evaluate((eventKinds) => {
    for (const kind of eventKinds) {
      if (kind === "resize") {
        window.dispatchEvent(new Event("resize"));
      } else if (kind === "orientationchange") {
        window.dispatchEvent(new Event("orientationchange"));
      } else {
        window.visualViewport?.dispatchEvent(new Event("resize"));
      }
    }
  }, events);
}

/** Two-finger outward pinch on the stable interaction surface (renderer-neutral). */
export async function syntheticOutwardPinchOnInteractionSurface(page: Page): Promise<void> {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate((el) => {
    const makePinch = (type: string, points: Array<{ x: number; y: number }>) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", {
        value: points.map((point) => ({ clientX: point.x, clientY: point.y })),
      });
      return event;
    };
    const rect = el.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top + rect.height / 2;
    // 100px start distance, spread past the 12px activation threshold.
    el.dispatchEvent(
      makePinch("touchstart", [
        { x: centerX - 50, y: centerY },
        { x: centerX + 50, y: centerY },
      ]),
    );
    el.dispatchEvent(
      makePinch("touchmove", [
        { x: centerX - 100, y: centerY },
        { x: centerX + 100, y: centerY },
      ]),
    );
    el.dispatchEvent(makePinch("touchend", []));
  });
}
