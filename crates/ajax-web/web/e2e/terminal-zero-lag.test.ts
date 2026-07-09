// Reproduces the zero-lag echo duplication seen on iOS Safari (screenshots
// "Text duplicate2" / "Duplicate text"): the operator types into an agent input
// box that redraws its line with escape sequences, so the predicted overlay
// ("pending") never literally matches the echo and lingers as a second, ghost
// copy of the typed text — e.g. `use play│use pla`.
//
// The fix (terminalZeroLag.ts backstops) must guarantee the ghost can never
// persist: it clears when the real echo advances the cursor, and unconditionally
// after the idle window.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

const gridCanvas = (page: Page) =>
  terminalPanel(page).locator("canvas:not([aria-hidden='true'])");

const zeroLagOverlay = (page: Page) =>
  terminalPanel(page).locator("[data-testid='terminal-zero-lag-input']");

/** Deliver a raw PTY output frame to the task terminal socket. */
async function emitTerminalOutput(page: Page, text: string) {
  const delivered = await page.evaluate((payload) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ url?: string; emitMessage(data: string): void }>;
      }
    ).__terminalSockets;
    const socket = [...sockets]
      .reverse()
      .find((item) => typeof item.url === "string" && item.url.includes("/terminal"));
    if (!socket) return false;
    socket.emitMessage(payload);
    return true;
  }, text);
  expect(delivered).toBe(true);
}

async function openTaskTerminal(page: Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(gridCanvas(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  // Land focus on the ghostty textarea so keystrokes drive the input path.
  await gridCanvas(page).click();
}

test("typed text paints a zero-lag prediction (reproduces the ghost)", async ({ page }) => {
  await openTaskTerminal(page);

  await page.keyboard.type("hello", { delay: 20 });

  // The prediction overlay is the ghost copy from the screenshots — it exists
  // to hide round-trip latency before the PTY echo arrives.
  await expect(zeroLagOverlay(page)).toHaveText("hello", { timeout: 2_000 });
});

test("prediction cannot persist as a duplicate — idle backstop clears it", async ({ page }) => {
  await openTaskTerminal(page);

  await page.keyboard.type("hello", { delay: 20 });
  await expect(zeroLagOverlay(page)).toHaveText("hello", { timeout: 2_000 });

  // Simulate the agent input box redrawing its line with escape sequences that
  // do NOT contain "hello" contiguously — the exact case the old literal-match
  // clear missed, leaving `use play│use pla`-style duplication.
  await emitTerminalOutput(page, "\x1b[2K\r\x1b[32m› \x1b[0mhe\x1b[1mllo\x1b[0m");

  // Backstops must remove the ghost: either the cursor advanced past the anchor,
  // or the 300ms idle timer fired. Either way, no lingering duplicate.
  await expect(zeroLagOverlay(page)).toHaveCount(0, { timeout: 2_000 });
});
