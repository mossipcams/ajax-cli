// Diff Review mobile-webkit smoke: task detail long-press + right swipe opens the
// projection shell without requiring a live Rust backend.

import { test, expect, type Locator } from "@playwright/test";
import { mockFetch, mockTerminalWebSocket } from "./fixtures";
import { NAVIGATE_LONG_PRESS_MS } from "../src/shared/gestures/navigateSwipe";

// Playwright requires object-destructured fixtures; empty pattern is intentional.
// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(async ({}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Diff Review entry swipe is a touch gesture; desktop has no equivalent",
  );
});

async function touchLongPressSwipeRight(target: Locator, dx: number) {
  await target.evaluate(
    async (el, { distance, holdMs }) => {
      const rect = el.getBoundingClientRect();
      const startX = rect.left + rect.width * 0.15;
      const startY = rect.top + rect.height / 2;
      const endX = startX + distance;
      const endY = startY;
      const make = (type: string, x: number, y: number) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, "touches", {
          value: [{ clientX: x, clientY: y }],
        });
        Object.defineProperty(event, "changedTouches", {
          value: [{ clientX: x, clientY: y }],
        });
        return event;
      };
      el.dispatchEvent(make("touchstart", startX, startY));
      await new Promise((resolve) => setTimeout(resolve, holdMs));
      el.dispatchEvent(make("touchmove", endX, endY));
      el.dispatchEvent(make("touchend", endX, endY));
    },
    { distance: dx, holdMs: NAVIGATE_LONG_PRESS_MS + 25 },
  );
}

test("task detail long-press swipe-right opens Diff Review chrome", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.getByTestId("task-detail")).toBeVisible({ timeout: 10_000 });

  await touchLongPressSwipeRight(page.getByTestId("mobile-chrome-header"), 120);
  await expect(page.getByTestId("outlet-diff")).toBeVisible();
  await expect(page.getByTestId("diff-review")).toBeVisible();
  await expect(page.getByTestId("diff-pr-strip")).toBeVisible();
  await expect(page.getByTestId("diff-file-list")).toBeVisible();
  await expect(page.getByTestId("diff-source")).toHaveText(/pr:12/);
});
