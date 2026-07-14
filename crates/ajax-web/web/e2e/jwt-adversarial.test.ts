// Adversarial JWT hunt: plant a compact JWT in API fields and terminal WS
// output the UI consumes, then fail if that canary becomes visible on client
// surfaces (localStorage, sessionStorage, URL, console, rendered HTML).
// Intentional API/WS capture buffers are injection channels — not pass/fail.

import { test, expect, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";
import {
  installJwtLeakProbe,
  snapshotBrowserSurfaces,
  collectContinuousFindings,
  assertNoJwtsOnSurfaces,
  CLIENT_JWT_SURFACES,
  type JwtFinding,
} from "./jwtLeakScan";

const CANARY =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhamF4LWNhbmFyeSJ9.dGVzdC1zaWc";

function hostileCockpit() {
  return {
    ...COCKPIT_FIXTURE,
    cards: COCKPIT_FIXTURE.cards.map((card) =>
      card.qualified_handle === "web/fix-login"
        ? { ...card, status_explanation: `Waiting for review ${CANARY}` }
        : card,
    ),
  };
}

function hostileDetail() {
  return {
    ...DETAIL_FIXTURE,
    status_explanation: `Waiting for review ${CANARY}`,
    title: `Fix login ${CANARY}`,
  };
}

/** Deliver a PTY output frame containing the canary to the task terminal socket. */
async function emitTerminalCanary(page: Page, token: string) {
  const payload = JSON.stringify({ type: "output", data: btoa(`leak ${token}\n`) });
  const delivered = await page.evaluate((frame) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ url?: string; emitMessage(data: string): void }>;
      }
    ).__terminalSockets;
    const socket = [...sockets]
      .reverse()
      .find((item) => typeof item.url === "string" && item.url.includes("/terminal"));
    if (!socket) return false;
    socket.emitMessage(frame);
    return true;
  }, payload);
  expect(delivered).toBe(true);
}

test("hostile API and WS JWTs must not appear on client surfaces", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  await mockFetch(page, {
    "/api/cockpit": hostileCockpit(),
    __detail__: hostileDetail(),
  });
  await mockTerminalWebSocket(page);
  const { consoleBuffer } = await installJwtLeakProbe(page);

  const findings: JwtFinding[] = [];

  // Dashboard — status_explanation with canary should render on inbox/card UI
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  findings.push(...(await snapshotBrowserSurfaces(page, "dashboard")));

  // Task detail via click — title/status_explanation carry canary
  await page.getByText("web/fix-login").click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await emitTerminalCanary(page, CANARY);
  await page.waitForTimeout(80);
  findings.push(...(await snapshotBrowserSurfaces(page, "task-detail")));

  findings.push(
    ...(await collectContinuousFindings(page, consoleBuffer, "continuous")),
  );

  // Transport buffers are expected to contain the plant; only client surfaces fail the hunt.
  assertNoJwtsOnSurfaces(findings, CLIENT_JWT_SURFACES);
});
