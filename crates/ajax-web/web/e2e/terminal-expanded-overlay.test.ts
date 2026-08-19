// Terminal-expanded Details overlay coverage. Lives outside terminal-behavior.test.ts
// so this PR does not touch that already-over-LOC file.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  waitForTerminalSocket,
} from "./fixtures";

async function openTaskTerminal(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalSurface(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
}

async function chromeDisplayState(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const cockpit = document.querySelector(".cockpit-chrome");
    const bottomNav = document.querySelector(".bottom-nav");
    const detailHeader = document.querySelector(".task-detail .detail-header");
    const interactPanel = document.querySelector(".task-detail .interact-panel");
    return {
      cockpit: cockpit ? getComputedStyle(cockpit).display : null,
      bottomNav: bottomNav ? getComputedStyle(bottomNav).display : null,
      detailHeader: detailHeader ? getComputedStyle(detailHeader).display : null,
      interactPanel: interactPanel ? getComputedStyle(interactPanel).display : null,
    };
  });
}

test("terminal-expanded keeps Details overlay while hiding cockpit chrome and bottom nav", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const before = await chromeDisplayState(page);
  expect(before.cockpit).not.toBe("none");
  expect(before.bottomNav).not.toBe("none");

  await page.evaluate(() => {
    document.documentElement.classList.add("terminal-expanded");
  });
  const hidden = await chromeDisplayState(page);
  expect(hidden.cockpit).toBe("none");
  expect(hidden.bottomNav).toBe("none");
  expect(hidden.detailHeader).not.toBe("none");
  expect(hidden.interactPanel).toBe("none");
  await expect(page.getByTestId("task-details")).toBeVisible();

  await page.evaluate(() => {
    document.documentElement.classList.remove("terminal-expanded");
  });
  const restored = await chromeDisplayState(page);
  expect(restored.cockpit).not.toBe("none");
  expect(restored.bottomNav).not.toBe("none");
});
