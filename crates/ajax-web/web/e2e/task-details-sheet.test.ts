// Task details sheet coverage superseding terminal-behavior.test.ts selectors.
// terminal-behavior.test.ts cannot be edited without tripping the 1000-line File LOC gate.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
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

const expandTerminalButton = (page: import("@playwright/test").Page) =>
  terminalSurface(page).getByRole("button", { name: "Expand terminal" });

test("phone fullscreen keeps background controls inert until task details sheet opens", async ({
  page,
}) => {
  await openTaskTerminal(page);
  const expandProbe = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');

  const backProbe = page.locator(".task-detail .back");
  const dashboardProbe = page.locator('.bottom-nav [data-bottom-route="#/"]');
  const detailsProbe = page.getByTestId("task-meta-details-trigger");
  const dismissProbe = page.locator(".result-panel button.pill");

  await page.locator("[data-action='review']").click();
  await expect(page.locator(".result-panel")).toBeVisible({ timeout: 10_000 });

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "true");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        header.inert &&
        chrome instanceof HTMLElement &&
        chrome.inert &&
        nav instanceof HTMLElement &&
        nav.inert &&
        meta instanceof HTMLElement &&
        meta.inert &&
        result instanceof HTMLElement &&
        result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(false);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(false);

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "false");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        !header.inert &&
        chrome instanceof HTMLElement &&
        !chrome.inert &&
        nav instanceof HTMLElement &&
        !nav.inert &&
        meta instanceof HTMLElement &&
        !meta.inert &&
        result instanceof HTMLElement &&
        !result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(true);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(true);

  await detailsProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(page.getByTestId("task-details-sheet")).toBeVisible();

  await dashboardProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(page.locator("[data-outlet='dashboard']")).toBeVisible({ timeout: 10_000 });
});

test("desktop expanded mode keeps terminal bounded and task details affordance reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const expand = expandTerminalButton(page);
  const maxInteractionHeight = Math.min(800 * 0.58, 560);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () =>
      terminalInteractionSurface(page).evaluate((el) => el.getBoundingClientRect().height),
    )
    .toBeLessThanOrEqual(maxInteractionHeight + 2);

  const detailsTrigger = page.getByTestId("task-meta-details-trigger");
  await detailsTrigger.scrollIntoViewIfNeeded();
  await expect(detailsTrigger).toBeInViewport();
});
