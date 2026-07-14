// JWT leak e2e for Web Cockpit. Two parts:
//   1. Detector self-check: plant a compact-JWT canary into every observable
//      surface (localStorage, sessionStorage, URL query, rendered HTML,
//      console, wrapped fetch body, wrapped WebSocket send+message) and
//      confirm the probe records each surface.
//   2. Clean exploration: drive the dashboard / task detail / settings /
//      new-task flows with mocked API + WS, snapshot browser surfaces after
//      every step, and assert zero JWT-shaped strings anywhere.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  terminalToolbar,
  waitForTerminalSocket,
} from "./fixtures";
import {
  installJwtLeakProbe,
  snapshotBrowserSurfaces,
  collectContinuousFindings,
  collectJwtFindings,
  assertNoJwts,
  type JwtFinding,
} from "./jwtLeakScan";

const CANARY =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhamF4LWNhbmFyeSJ9.dGVzdC1zaWc";

// Sets up a fetch route for the canary probe path. Must be installed BEFORE the
// probe so the probe wrapper records the response body as it flows through.
async function installCanaryFetchRoute(page: import("@playwright/test").Page) {
  await page.addInitScript((token: string) => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/jwt-canary") {
        return new Response(
          JSON.stringify({ token, note: "probe-only" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return orig(input, init);
    };
  }, CANARY);
}

test("jwt leak probe detects a canary in every observable surface", async ({ page }) => {
  // Order: fetch mock -> WS mock -> canary-fetch route -> probe. The probe's
  // re-armer wraps the current fetch/WebSocket regardless of ordering, but we
  // pick a concrete order so the canary response flows through the probe.
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await installCanaryFetchRoute(page);
  const { consoleBuffer } = await installJwtLeakProbe(page);

  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Plant the canary into each surface.
  await page.evaluate((token: string) => {
    localStorage.setItem("canary-storage", token);
    sessionStorage.setItem("canary-session", token);
    history.replaceState(
      null,
      "",
      `/app.html?probe=${encodeURIComponent(token)}`,
    );
    document.body.appendChild(document.createTextNode(token));
    console.log(token);
    void fetch("/api/jwt-canary");

    // WebSocket: the mock's constructor pushes the (wrapped) instance into
    // __terminalSockets. Probe records both send and inbound message.
    const ws = new WebSocket("ws://localhost/canary");
    try {
      ws.send(token);
    } catch {
      // ignore send-state edge cases
    }
    const sockets = (
      window as unknown as { __terminalSockets: Array<{ emitMessage: (d: string) => void }> }
    ).__terminalSockets;
    sockets.at(-1)?.emitMessage(token);
  }, CANARY);

  // Drain a couple of re-arm ticks so probe wrappers settle on the latest
  // mocked globals. We also allow the WS message event to flush.
  await page.waitForTimeout(80);

  const findings = await collectJwtFindings(page, consoleBuffer, "self-check");

  const surfaces = new Set(findings.map((f: JwtFinding) => f.surface));
  const expected: JwtFinding["surface"][] = [
    "localStorage",
    "sessionStorage",
    "url",
    "html",
    "console",
    "api",
    "websocket",
  ];
  for (const surface of expected) {
    expect(surfaces, `canary not detected on surface: ${surface}`).toContain(surface);
  }
});

test("clean exploration of web cockpit surfaces zero JWT findings", async ({ page }) => {
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  const { consoleBuffer } = await installJwtLeakProbe(page);

  const findings: JwtFinding[] = [];

  // 1. Dashboard
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  findings.push(...(await snapshotBrowserSurfaces(page, "dashboard")));

  // 2. Task detail (with terminal + toolbar Esc)
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await terminalToolbar(page).getByRole("button", { name: "Esc" }).click();
  await page.waitForTimeout(60);
  findings.push(...(await snapshotBrowserSurfaces(page, "task-detail")));

  // 3. Settings
  await page.goto("/app.html#/settings");
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({
    timeout: 5_000,
  });
  findings.push(...(await snapshotBrowserSurfaces(page, "settings")));

  // 4. New task sheet
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  await expect(page.locator("#new-task-title-input")).toBeVisible();
  findings.push(...(await snapshotBrowserSurfaces(page, "new-task")));

  // Continuous captures (console + API bodies + WS messages) accumulated across
  // all steps must be merged before the final assertion.
  findings.push(
    ...(await collectContinuousFindings(page, consoleBuffer, "continuous")),
  );

  assertNoJwts(findings);
});