// Sibling hunts for C4/C5-shaped defects (mobile-webkit).
// C6: host height changes under keyboard-open (reconnect status, paste fallback)
//     deepen the frozen crop — same allowLocalFit=false root cause as C4.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  terminalToolbar,
  waitForTerminalSocket,
} from "./fixtures";

test.beforeEach(async ({}, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "C4/C5 siblings: WebKit only");
});

async function installVV(page: Page) {
  await page.addInitScript(() => {
    const listeners: Record<string, Array<() => void>> = {};
    const vv = {
      height: 844,
      width: 390,
      offsetTop: 0,
      offsetLeft: 0,
      scale: 1,
      addEventListener(type: string, fn: () => void) {
        (listeners[type] ??= []).push(fn);
      },
      removeEventListener(type: string, fn: () => void) {
        listeners[type] = (listeners[type] ?? []).filter((x) => x !== fn);
      },
      dispatch(type: string) {
        for (const fn of listeners[type] ?? []) fn();
      },
    };
    Object.defineProperty(window, "visualViewport", {
      configurable: true,
      get: () => vv,
    });
    (window as unknown as { __setVV: (h: number, t: number) => void }).__setVV = (nh, nt) => {
      vv.height = nh;
      vv.offsetTop = nt;
      vv.dispatch("resize");
    };
  });
}

async function setVV(page: Page, height: number, offsetTop = 0) {
  await page.evaluate(
    ({ height: h, offsetTop: t }) => {
      (window as unknown as { __setVV: (h: number, t: number) => void }).__setVV(h, t);
    },
    { height, offsetTop },
  );
}

async function openTask(page: Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await installVV(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(
    terminalPanel(page).locator("canvas:not([aria-hidden='true'])"),
  ).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await page.waitForTimeout(400);
}

async function cropMetrics(page: Page) {
  return page.evaluate(() => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as HTMLElement;
    const canvas = document.querySelector(
      "[data-testid='task-terminal-panel'] canvas:not([aria-hidden='true'])",
    ) as HTMLElement;
    const status = document.querySelector("[data-testid='terminal-status']") as HTMLElement | null;
    const hr = host.getBoundingClientRect();
    const cr = canvas.getBoundingClientRect();
    return {
      kb: document.documentElement.classList.contains("keyboard-open"),
      hostH: hr.height,
      canvasAbove: Math.max(0, hr.top - cr.top),
      fillY: cr.height / Math.max(1, hr.height),
      statusEmpty: status?.classList.contains("is-empty") ?? null,
      statusH: status?.getBoundingClientRect().height ?? 0,
      statusText: (status?.textContent || "").trim().slice(0, 60),
    };
  });
}

test("C6: reconnect status under keyboard must not deepen the crop blank", async ({ page }) => {
  await openTask(page);
  await setVV(page, 420, 48);
  await page.waitForTimeout(300);
  const before = await cropMetrics(page);
  expect(before.kb).toBe(true);

  await page.evaluate(() => {
    (
      window as unknown as { __terminalSockets: Array<{ emitClose(): void }> }
    ).__terminalSockets.at(-1)?.emitClose();
  });
  await page.waitForTimeout(250);
  const during = await cropMetrics(page);

  expect(during.statusEmpty, "reconnect should show status").toBe(false);
  expect(
    during.canvasAbove - before.canvasAbove,
    `reconnect deepened crop by ${during.canvasAbove - before.canvasAbove}px (before=${before.canvasAbove} during=${during.canvasAbove}, hostH ${before.hostH}→${during.hostH})`,
  ).toBeLessThanOrEqual(8);
  expect(during.canvasAbove, `absolute crop ${during.canvasAbove}`).toBeLessThanOrEqual(24);
});

test("C6: paste fallback under keyboard must not deepen the crop blank", async ({ page }) => {
  await openTask(page);
  // fixtures mock a working clipboard; break it so Paste opens the fallback UI.
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        readText: async () => {
          throw new Error("clipboard unavailable");
        },
      },
    });
  });

  await setVV(page, 420, 48);
  await page.waitForTimeout(250);
  const before = await cropMetrics(page);

  await page.evaluate(() => {
    const row = document.querySelector(".terminal-keys") as HTMLElement;
    row.scrollLeft = row.scrollWidth;
  });
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByTestId("terminal-paste-fallback")).toBeVisible({ timeout: 3_000 });
  await page.waitForTimeout(200);
  const after = await cropMetrics(page);

  expect(
    after.canvasAbove - before.canvasAbove,
    `paste fallback deepened crop by ${after.canvasAbove - before.canvasAbove}px (before=${before.canvasAbove} after=${after.canvasAbove}, hostH ${before.hostH}→${after.hostH})`,
  ).toBeLessThanOrEqual(8);
  expect(after.canvasAbove, `absolute crop ${after.canvasAbove}`).toBeLessThanOrEqual(24);
});

test("NOTE: keyboard height settle must not grow crop (C4 aggravation)", async ({ page }) => {
  await openTask(page);
  await setVV(page, 480, 40);
  await page.waitForTimeout(200);
  const a = await cropMetrics(page);
  await setVV(page, 440, 44);
  await page.waitForTimeout(80);
  await setVV(page, 400, 48);
  await page.waitForTimeout(80);
  await setVV(page, 420, 48);
  await page.waitForTimeout(250);
  const b = await cropMetrics(page);

  expect(b.kb).toBe(true);
  expect(
    b.canvasAbove - a.canvasAbove,
    `keyboard settle grew crop by ${b.canvasAbove - a.canvasAbove}px (${a.canvasAbove}→${b.canvasAbove})`,
  ).toBeLessThanOrEqual(16);
});
