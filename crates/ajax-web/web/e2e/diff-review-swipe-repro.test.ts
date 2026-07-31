// Mobile-webkit: plain swipe-left opens Diff; swipe-right does not.
//
//   npm run web:smoke -- e2e/diff-review-swipe-repro.test.ts

import { test, expect, type Locator } from "@playwright/test";
import { mockFetch, mockTerminalWebSocket } from "./fixtures";

// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(async ({}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Diff Review entry swipe is a touch gesture; desktop has no equivalent",
  );
});

async function dispatchTouch(
  locator: Locator,
  type: "touchstart" | "touchmove" | "touchend",
  x: number,
  y: number,
) {
  await locator.evaluate(
    (el, args) => {
      const event = new Event(args.type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", {
        value: [{ clientX: args.x, clientY: args.y }],
      });
      Object.defineProperty(event, "changedTouches", {
        value: [{ clientX: args.x, clientY: args.y }],
      });
      el.dispatchEvent(event);
    },
    { type, x, y },
  );
}

async function headerPoint(page: import("@playwright/test").Page) {
  const header = page.getByTestId("mobile-chrome-header");
  await expect(page.getByTestId("task-detail")).toBeVisible({ timeout: 10_000 });
  const box = await header.boundingBox();
  if (!box) throw new Error("header missing box");
  return {
    header,
    x: box.x + box.width * 0.15,
    y: box.y + box.height / 2,
  };
}

test("swipe-left opens Diff", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  const { header, x, y } = await headerPoint(page);

  await dispatchTouch(header, "touchstart", x + 120, y);
  await dispatchTouch(header, "touchmove", x, y);
  await dispatchTouch(header, "touchend", x, y);

  await expect(page.getByTestId("outlet-diff")).toBeVisible({ timeout: 5000 });
});

test("swipe-right does not open Diff", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  const { header, x, y } = await headerPoint(page);

  await dispatchTouch(header, "touchstart", x, y);
  await dispatchTouch(header, "touchmove", x + 120, y);
  await dispatchTouch(header, "touchend", x + 120, y);

  await expect(page.getByTestId("outlet-diff")).toHaveCount(0);
  await expect(page.getByTestId("task-detail")).toHaveCount(0);
  await expect(page.getByTestId("outlet-dashboard")).toBeVisible();
});
