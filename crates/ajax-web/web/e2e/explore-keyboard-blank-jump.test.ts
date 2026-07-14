// Soft-keyboard layout defects (mobile-webkit).
// C4: keyboard-open freezes fit + cropToBottom → fresh CLI prompt scrolled off;
//     empty PTY rows fill the band above the key bar.
// C5: keyboard dismiss restores chrome → terminal host jumps down ~100px.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

test.beforeEach(async ({}, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "keyboard blank/jump: WebKit only");
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
  await page.waitForTimeout(500);
}

async function geometry(page: Page) {
  return page.evaluate(() => {
    const panel = document.querySelector("[data-testid='task-terminal-panel']") as HTMLElement;
    const host = panel.querySelector(".terminal-host") as HTMLElement;
    const canvasEl = panel.querySelector("canvas:not([aria-hidden='true'])") as HTMLElement;
    const keys = panel.querySelector(".terminal-keys") as HTMLElement;
    const hr = host.getBoundingClientRect();
    const cr = canvasEl.getBoundingClientRect();
    const kr = keys.getBoundingClientRect();
    return {
      keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      hostTop: hr.top,
      hostH: hr.height,
      canvasTop: cr.top,
      canvasH: cr.height,
      canvasAboveHost: Math.max(0, hr.top - cr.top),
      blankBelow: Math.max(0, hr.bottom - cr.bottom),
      fillY: cr.height / Math.max(1, hr.height),
      keysAttached: Math.abs(kr.bottom - (hr.bottom + (kr.top - hr.bottom) + kr.height)) < 999,
      hostToKeys: kr.top - hr.bottom,
      headerDisplay: getComputedStyle(
        document.querySelector(".detail-header") as HTMLElement,
      ).display,
    };
  });
}

test("C4: soft keyboard must not leave a cropped empty band above the keys", async ({ page }) => {
  await openTask(page);
  const before = await geometry(page);
  expect(before.canvasAboveHost).toBeLessThanOrEqual(8);

  // Operator focuses the PTY — iOS soft keyboard collapses visualViewport.
  await terminalPanel(page)
    .locator("canvas:not([aria-hidden='true'])")
    .click({ position: { x: 40, y: 40 } });
  await setVV(page, 420, 48);
  await page.waitForTimeout(400);

  const open = await geometry(page);
  expect(open.keyboardOpen, "viewport mock should flag keyboard-open").toBe(true);
  expect(open.headerDisplay, "chrome should collapse under keyboard").toBe("none");

  // Defect: fit is frozen; pre-keyboard canvas stays tall and is crop-scrolled,
  // so the top of the grid (fresh CLI prompt) sits above the host while empty
  // rows fill the visible band above the key bar.
  expect(
    open.canvasAboveHost,
    `canvas cropped above host by ${open.canvasAboveHost}px (prompt off-screen; empty rows above keys)`,
  ).toBeLessThanOrEqual(24);
  expect(
    open.fillY,
    `oversized canvas fillY=${open.fillY} — fit should have reflowed to the keyboard band`,
  ).toBeLessThanOrEqual(1.08);
});

test("C5: dismissing the soft keyboard must not jump the terminal down", async ({ page }) => {
  await openTask(page);
  await terminalPanel(page)
    .locator("canvas:not([aria-hidden='true'])")
    .click({ position: { x: 40, y: 40 } });
  await setVV(page, 420, 48);
  await page.waitForTimeout(300);
  const open = await geometry(page);
  expect(open.keyboardOpen).toBe(true);

  // "A few seconds of use" then keyboard dismisses (Hide keyboard / iOS settle).
  await page.waitForTimeout(2500);
  await setVV(page, 844, 0);
  await page.waitForTimeout(500);
  const closed = await geometry(page);
  expect(closed.keyboardOpen).toBe(false);

  const jumpDown = closed.hostTop - open.hostTop;
  expect(
    jumpDown,
    `terminal host jumped down ${jumpDown}px on keyboard dismiss (open@${open.hostTop} → closed@${closed.hostTop})`,
  ).toBeLessThanOrEqual(24);
});

test("C4/C5: new-task handoff with keyboard still open then dismiss", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installVV(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // New Task sheet: soft keyboard up, then Start navigates to the task.
  await setVV(page, 420, 40);
  await page.waitForTimeout(150);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await page.waitForTimeout(700);

  const open = await geometry(page);
  expect(open.keyboardOpen).toBe(true);
  expect(
    open.canvasAboveHost,
    `new-task handoff cropped canvas by ${open.canvasAboveHost}px`,
  ).toBeLessThanOrEqual(24);
  expect(open.fillY, `new-task handoff fillY=${open.fillY}`).toBeLessThanOrEqual(1.08);

  await page.waitForTimeout(2000);
  await setVV(page, 844, 0);
  await page.waitForTimeout(500);
  const closed = await geometry(page);
  const jumpDown = closed.hostTop - open.hostTop;
  expect(
    jumpDown,
    `new-task keyboard dismiss jumped host down ${jumpDown}px`,
  ).toBeLessThanOrEqual(24);
});
