// JWT leak probe for the Web Cockpit e2e suite. Installs order-independent
// wrappers around fetch and WebSocket in the page, snapshots browser surfaces
// (localStorage, sessionStorage, URL, rendered HTML) per route, and surfaces a
// single JwtFinding[] stream for an assert helper. No production behavior is
// touched — this module is consumed only by e2e tests.

import { expect, type Page } from "@playwright/test";

// Compact JWT: three base64url segments starting with `eyJ`. We accept the
// common HS256/RS256 header prefix; the rest is base64url over some length.
const JWT_RE = /\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\b/g;

export type JwtSurface =
  | "localStorage"
  | "sessionStorage"
  | "url"
  | "html"
  | "console"
  | "api"
  | "websocket";

export interface JwtFinding {
  surface: JwtSurface;
  label: string;
  snippet: string;
}

function scan(text: string): string[] {
  const out: string[] = [];
  if (!text) return out;
  JWT_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = JWT_RE.exec(text)) !== null) {
    out.push(m[0]);
    if (m.index === JWT_RE.lastIndex) JWT_RE.lastIndex++; // avoid zero-length loop
  }
  return out;
}

// ---- probe install --------------------------------------------------------

export async function installJwtLeakProbe(page: Page) {
  // Node-side console buffer. We do not require an in-page hook because the
  // Playwright `console` event already intercepts every page console call.
  const consoleBuffer: string[] = [];
  page.on("console", (msg) => {
    const text = msg.text();
    consoleBuffer.push(text);
  });

  await page.addInitScript(() => {
    const API = "__ajaxJwtApiBodies" as const;
    const WS = "__ajaxJwtWsMessages" as const;
    (window as unknown as Record<string, unknown[]>)[API] ??= [];
    (window as unknown as Record<string, unknown[]>)[WS] ??= [];

    const MARK_FETCH = "__ajaxJwtProbeFetch" as const;
    const MARK_WS = "__ajaxJwtProbeWs" as const;

    const decodeBuf = (d: unknown): string => {
      if (typeof d === "string") return d;
      if (d instanceof ArrayBuffer) return new TextDecoder().decode(d);
      if (ArrayBuffer.isView(d as ArrayBufferView)) {
        const v = d as ArrayBufferView;
        return new TextDecoder().decode(
          new Uint8Array(v.buffer, v.byteOffset, v.byteLength),
        );
      }
      return "";
    };

    const wrapFetch = () => {
      const cur = globalThis.fetch as
        | ((input: RequestInfo | URL, init?: RequestInit) => Promise<Response>)
        | undefined;
      if (!cur) return;
      const anyCur = cur as unknown as Record<string, unknown>;
      if (anyCur[MARK_FETCH]) return;
      const apiBodies = (window as unknown as Record<string, unknown[]>)[API];
      const wrapped = async function (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> {
        const res = await cur.call(globalThis, input, init);
        try {
          const url =
            typeof input === "string"
              ? input
              : input instanceof URL
                ? input.href
                : (input as Request).url;
          const clone = res.clone();
          const body = await clone.text();
          apiBodies.push({ path: url, body });
        } catch {
          // never break consumers on probe failure
        }
        return res;
      };
      Object.defineProperty(wrapped, MARK_FETCH, { value: true });
      Object.defineProperty(wrapped, "name", { value: "ajaxJwtProbeFetch" });
      globalThis.fetch = wrapped as typeof globalThis.fetch;
    };

    const wrapWS = () => {
      const cur = globalThis.WebSocket as
        | (new (url: string | URL, protocols?: string | string[]) => WebSocket)
        | undefined;
      if (!cur) return;
      const anyCur = cur as unknown as Record<string, unknown>;
      if (anyCur[MARK_WS]) return;
      const wsMessages = (window as unknown as Record<string, unknown[]>)[WS];
      class WrappedWebSocket extends cur {
        constructor(url: string | URL, protocols?: string | string[]) {
          super(url, protocols);
          try {
            this.addEventListener("message", (e: MessageEvent) => {
              const data = decodeBuf((e as MessageEvent).data);
              if (data) wsMessages.push({ dir: "recv", url: String(url), data });
            });
          } catch {
            // ignore attach failures
          }
        }
        send(data: string | ArrayBuffer | ArrayBufferView) {
          const text = decodeBuf(data);
          if (text) wsMessages.push({ dir: "send", url: this.url, data: text });
          try {
            super.send(data as string);
          } catch {
            // mock may reject non-JSON payloads; recording already happened
          }
        }
      }
      Object.defineProperty(WrappedWebSocket, MARK_WS, { value: true });
      globalThis.WebSocket = WrappedWebSocket as typeof globalThis.WebSocket;
    };

    const arm = () => {
      wrapFetch();
      wrapWS();
    };
    arm();
    queueMicrotask(arm);
    const timer = setInterval(arm, 50);
    // Clean up the re-armer when the page is torn down so long-running test
    // sessions do not leak timers across navigations.
    window.addEventListener("pagehide", () => clearInterval(timer), {
      once: true,
    });
  });

  return { consoleBuffer };
}

// ---- continuous capture aggregation --------------------------------------

export async function collectContinuousFindings(
  page: Page,
  consoleBuffer: string[],
  label: string,
): Promise<JwtFinding[]> {
  const out: JwtFinding[] = [];
  for (const line of consoleBuffer) {
    for (const jwt of scan(line)) {
      out.push({ surface: "console", label, snippet: jwt });
    }
  }
  const captured = await page.evaluate(() => {
    const api = (window as unknown as Record<string, Array<{ path: string; body: string }>>)
      .__ajaxJwtApiBodies ?? [];
    const ws = (window as unknown as Record<string, Array<{ dir: string; url: string; data: string }>>)
      .__ajaxJwtWsMessages ?? [];
    return {
      api: api.map((e) => ({ path: e.path, body: e.body })),
      ws: ws.map((e) => ({ dir: e.dir, url: e.url, data: e.data })),
    };
  });
  for (const entry of captured.api) {
    for (const jwt of scan(entry.body)) {
      out.push({ surface: "api", label, snippet: jwt });
    }
  }
  for (const entry of captured.ws) {
    for (const jwt of scan(entry.data)) {
      out.push({ surface: "websocket", label, snippet: jwt });
    }
  }
  return out;
}

// ---- per-route surface snapshot ------------------------------------------

export async function snapshotBrowserSurfaces(
  page: Page,
  label: string,
): Promise<JwtFinding[]> {
  const out: JwtFinding[] = [];
  const snap = await page.evaluate(() => {
    const local: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k == null) continue;
      local.push(`${k}=${localStorage.getItem(k) ?? ""}`);
    }
    const session: string[] = [];
    for (let i = 0; i < sessionStorage.length; i++) {
      const k = sessionStorage.key(i);
      if (k == null) continue;
      session.push(`${k}=${sessionStorage.getItem(k) ?? ""}`);
    }
    return {
      local: local.join("\n"),
      session: session.join("\n"),
      url: location.href,
      html: document.documentElement.outerHTML,
    };
  });

  for (const jwt of scan(snap.local)) {
    out.push({ surface: "localStorage", label, snippet: jwt });
  }
  for (const jwt of scan(snap.session)) {
    out.push({ surface: "sessionStorage", label, snippet: jwt });
  }
  for (const jwt of scan(snap.url)) {
    out.push({ surface: "url", label, snippet: jwt });
  }
  for (const jwt of scan(snap.html)) {
    out.push({ surface: "html", label, snippet: jwt });
  }
  return out;
}

// ---- final assertion ------------------------------------------------------

/** Client-visible surfaces for adversarial hunts (excludes intentional API/WS injection). */
export const CLIENT_JWT_SURFACES: readonly JwtSurface[] = [
  "localStorage",
  "sessionStorage",
  "url",
  "html",
  "console",
] as const;

export function assertNoJwts(findings: JwtFinding[]) {
  assertNoJwtsOnSurfaces(findings, [
    "localStorage",
    "sessionStorage",
    "url",
    "html",
    "console",
    "api",
    "websocket",
  ]);
}

/** Fail only on the named surfaces (e.g. client-only after hostile API/WS plant). */
export function assertNoJwtsOnSurfaces(
  findings: JwtFinding[],
  surfaces: readonly JwtSurface[],
) {
  const allowed = new Set(surfaces);
  const filtered = findings.filter((f) => allowed.has(f.surface));
  if (filtered.length === 0) return;
  const grouped = filtered
    .map((f) => `  [${f.surface}] (${f.label}) ${f.snippet}`)
    .join("\n");
  expect(
    filtered,
    `JWT-shaped strings leaked into observable surfaces:\n${grouped}`,
  ).toHaveLength(0);
}

// Convenience: full single-route aggregation (not used by the multi-step test,
// but handy for self-checks that stay on one route).
export async function collectJwtFindings(
  page: Page,
  consoleBuffer: string[],
  label: string,
): Promise<JwtFinding[]> {
  const snap = await snapshotBrowserSurfaces(page, label);
  const cont = await collectContinuousFindings(page, consoleBuffer, label);
  return [...snap, ...cont];
}