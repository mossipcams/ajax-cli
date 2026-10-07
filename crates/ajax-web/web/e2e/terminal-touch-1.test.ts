// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.


import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalToolbar,
  failLatestTerminalSocket,
  emitLatestTerminalOutput,
  waitForTerminalSocket
} from "./fixtures";
import {
  COMPACT_KEY_MAX_PX,
  COMPACT_KEY_MIN_PX,
  PRIMARY_TOUCH_MIN_PX,
  gotoTaskRoute,
  newOutputButton,
  openTaskTerminal,
  scrollInteractionSurfaceAway,
  scrollbackChunk,
  waitForSeedRevealSettled
} from "./terminal-behavior-helpers";


// Playwright requires object-destructured fixtures; empty pattern is intentional.
// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(({}, testInfo) => {
  const desktopOnly =
    testInfo.title ===
    "desktop expanded mode keeps terminal bounded and task details summary reachable";
  if (desktopOnly) {
    test.skip(
      testInfo.project.name !== "desktop-chromium",
      "desktop expanded layout only",
    );
  } else {
    test.skip(
      testInfo.project.name !== "mobile-webkit",
      "terminal acceptance is mobile-webkit only",
    );
  }
});


test("compact terminal keys are smaller than primary touch targets on phone", async ({ page }) => {
  await openTaskTerminal(page);

  const keySizes = await page.evaluate(() =>
    Array.from(document.querySelectorAll(".terminal-keys .terminal-key")).map((el) => {
      const rect = (el as HTMLElement).getBoundingClientRect();
      return { width: rect.width, height: rect.height };
    }),
  );
  expect(keySizes.length).toBeGreaterThan(0);
  for (const size of keySizes) {
    expect(size.height).toBeGreaterThanOrEqual(COMPACT_KEY_MIN_PX);
    expect(size.height).toBeLessThanOrEqual(COMPACT_KEY_MAX_PX);
    expect(size.height).toBeLessThan(PRIMARY_TOUCH_MIN_PX);
  }
});


test("terminal controls meet mobile touch target size on phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const measureVisibleTerminalButtons = () =>
    page.evaluate(() => {
      const panel = document.querySelector("[data-testid='task-terminal-panel']");
      if (!panel) throw new Error("terminal panel missing");
      const measured: Array<{ width: number; height: number; selector: string }> = [];
      const selectors = [
        ".terminal-expand-corner",
        ".terminal-keys .terminal-key",
        ".terminal-new-output",
        ".terminal-status-reconnect",
        ".terminal-paste-actions .terminal-key",
      ];
      for (const selector of selectors) {
        for (const el of panel.querySelectorAll(selector)) {
          const rect = (el as HTMLElement).getBoundingClientRect();
          measured.push({
            selector,
            width: rect.width,
            height: rect.height,
          });
        }
      }
      return measured;
    });

  const expectPrimaryTouchTargets = (
    sizes: Array<{ width: number; height: number; selector: string }>,
    requiredSelectors: string[],
  ) => {
    expect(sizes.length).toBeGreaterThan(0);
    for (const size of sizes) {
      if (size.selector === ".terminal-keys .terminal-key") {
        expect(size.height).toBeGreaterThanOrEqual(COMPACT_KEY_MIN_PX);
        expect(size.height).toBeLessThanOrEqual(COMPACT_KEY_MAX_PX);
        continue;
      }
      expect(size.width).toBeGreaterThanOrEqual(PRIMARY_TOUCH_MIN_PX);
      expect(size.height).toBeGreaterThanOrEqual(PRIMARY_TOUCH_MIN_PX);
    }
    for (const selector of requiredSelectors) {
      expect(sizes.some((size) => size.selector === selector)).toBe(true);
    }
  };

  let sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-expand-corner", ".terminal-keys .terminal-key"]);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await waitForSeedRevealSettled(page);
  await scrollInteractionSurfaceAway(page);
  await emitLatestTerminalOutput(page, ["more output\r\n"]);
  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-new-output"]);

  await failLatestTerminalSocket(page, "tmux session missing");
  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-status-reconnect"]);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expect(
    sizes.filter((size) => size.selector === ".terminal-paste-actions .terminal-key").length,
  ).toBe(2);
  expectPrimaryTouchTargets(sizes, [".terminal-paste-actions .terminal-key"]);
});
