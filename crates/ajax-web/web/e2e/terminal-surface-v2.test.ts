// Surface V2 (wterm) must mount on mobile WebKit without the yellow init banner.
// This is the bake-off gate CI was missing — unit/jsdom tests stayed green while
// device Safari failed. Init failure unmounts the panel and shows the banner, so
// settle on either outcome before asserting success.

import { test, expect } from "@playwright/test";
import { mockFetch, mockTerminalWebSocket, waitForTerminalSocket } from "./fixtures";

async function enableSurfaceV2(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem("ajax.terminal.surfaceV2", "true");
  });
}

async function surfaceV2FailureContext(page: import("@playwright/test").Page) {
  return page.evaluate(() => ({
    banner: document.querySelector('[data-testid="terminal-surface-v2-error"]')?.textContent ?? null,
    lastError: sessionStorage.getItem("ajax.terminal.surfaceV2.lastError"),
    engines: [...document.querySelectorAll("[data-terminal-engine]")].map((el) =>
      el.getAttribute("data-terminal-engine"),
    ),
  }));
}

test("Surface V2 mounts wterm on mobile webkit without yellow init failure", async ({
  page,
}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Safari/WebKit is the bake-off target for Surface V2",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(String(err)));
  page.on("console", (msg) => {
    if (msg.type() === "error") pageErrors.push(`console: ${msg.text()}`);
  });

  await page.goto("/app.html#/t/web%2Ffix-login");

  const errorBanner = page.getByTestId("terminal-surface-v2-error");
  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  const termGrid = wtermPanel.locator(".term-grid");

  // Init failure swaps the panel for the yellow banner — wait for a settled outcome.
  await Promise.race([
    termGrid.waitFor({ state: "visible", timeout: 20_000 }),
    errorBanner.waitFor({ state: "visible", timeout: 20_000 }),
  ]).catch(async () => {
    const ctx = await surfaceV2FailureContext(page);
    throw new Error(
      `Surface V2 never settled (no .term-grid, no yellow banner).\n` +
        `context=${JSON.stringify(ctx)}\npageErrors=${JSON.stringify(pageErrors)}`,
    );
  });

  if (await errorBanner.isVisible().catch(() => false)) {
    const ctx = await surfaceV2FailureContext(page);
    throw new Error(
      `Surface V2 yellow banner still showing.\n` +
        `context=${JSON.stringify(ctx)}\npageErrors=${JSON.stringify(pageErrors)}`,
    );
  }

  await expect(errorBanner).toHaveCount(0);
  await expect(wtermPanel).toBeVisible();
  await expect(termGrid).toBeVisible();
  await waitForTerminalSocket(page);

  // Host must stay dark paper — solid mustard/olive fill is the device yellow bug.
  const hostBg = await page.evaluate(() => {
    const host = document.querySelector(".wterm-host");
    return host ? getComputedStyle(host).backgroundColor : null;
  });
  expect(hostBg).toMatch(/rgba?\(\s*28\s*,\s*23\s*,\s*20/);

  await page.evaluate(() => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ emitMessage: (d: string) => void }>;
      }
    ).__terminalSockets;
    sockets[sockets.length - 1].emitMessage("Hello from Surface V2\r\n");
  });

  await expect
    .poll(async () => (await wtermPanel.textContent()) ?? "", { timeout: 10_000 })
    .toContain("Hello from Surface V2");
});

test("Surface V2 stays off Ghostty when the flag is enabled", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const errorBanner = page.getByTestId("terminal-surface-v2-error");
  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );

  await Promise.race([
    wtermPanel.locator(".term-grid").waitFor({ state: "visible", timeout: 20_000 }),
    errorBanner.waitFor({ state: "visible", timeout: 20_000 }),
  ]);

  await expect(errorBanner).toHaveCount(0);
  await expect(wtermPanel).toBeVisible();
  await expect(page.locator('[data-terminal-engine="ghostty"]')).toHaveCount(0);
});
