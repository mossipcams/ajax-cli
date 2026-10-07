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
  terminalInputFrames,
  waitForTerminalSocket
} from "./fixtures";
import {
  BACKSPACE,
  BACK_LEFT,
  KEY_REPEAT_HOLD_MS,
  clickTerminalSurfaceInterior,
  gotoTaskRoute,
  holdToolbarButton,
  inputFrameCount,
  openTaskTerminal,
  settleNoNewFrames,
  tapToolbarButton
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


test("printable, control, and navigation keys produce ordered PTY input", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);
  const toolbar = terminalToolbar(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("abc");
  await page.keyboard.press("Enter");

  await toolbar.getByRole("button", { name: "Tab" }).click();
  await toolbar.getByRole("button", { name: "Escape" }).click();
  await toolbar.getByRole("button", { name: "Left arrow" }).click();
  await toolbar.getByRole("button", { name: "Up arrow" }).click();
  await toolbar.getByRole("button", { name: "Down arrow" }).click();
  await toolbar.getByRole("button", { name: "Right arrow" }).click();

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual([
      "a",
      "b",
      "c",
      "\r",
      "\t",
      "\x1b",
      "\x1b[D",
      "\x1b[A",
      "\x1b[B",
      "\x1b[C",
    ]);
});


test("held terminal back repeats left-arrow frames then stops on release", async ({ page }) => {
  await openTaskTerminal(page);
  const back = terminalToolbar(page).getByRole("button", { name: "Left arrow" });
  const baseline = await inputFrameCount(page);

  await holdToolbarButton(page, "Left arrow", KEY_REPEAT_HOLD_MS);
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBeGreaterThanOrEqual(2);
  const frames = (await terminalInputFrames(page)).slice(baseline);
  expect(frames.every((frame) => frame.data === BACK_LEFT)).toBe(true);
  const afterHold = await inputFrameCount(page);
  await settleNoNewFrames(page, afterHold);

  await back.focus();
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(afterHold - baseline + 1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe(BACK_LEFT);
});


test("Backspace tap sends one DEL frame", async ({ page }) => {
  await openTaskTerminal(page);
  const baseline = await inputFrameCount(page);

  await tapToolbarButton(page, "Backspace");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe(BACKSPACE);
  await settleNoNewFrames(page, baseline + 1);
});


test("repeatable hotbar key ignores a late trailing pointer click without double-sending", async ({
  page,
}) => {
  await openTaskTerminal(page);
  const baseline = await inputFrameCount(page);

  // A tap already emits once on pointerdown. iOS can then deliver the synthetic
  // compat click a frame late — after the old setTimeout(0) suppress flag had
  // expired — which re-sent the arrow and skipped a line. A pointer-generated
  // click carries detail > 0; a keyboard activation carries detail 0.
  await tapToolbarButton(page, "Left arrow");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);

  await terminalToolbar(page)
    .getByRole("button", { name: "Left arrow" })
    .dispatchEvent("click", { detail: 1 });

  await settleNoNewFrames(page, baseline + 1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe(BACK_LEFT);
});


test("held Backspace repeats DEL frames then stops on release", async ({ page }) => {
  await openTaskTerminal(page);
  const baseline = await inputFrameCount(page);

  await holdToolbarButton(page, "Backspace", KEY_REPEAT_HOLD_MS);
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBeGreaterThanOrEqual(2);
  const frames = (await terminalInputFrames(page)).slice(baseline);
  expect(frames.every((frame) => frame.data === BACKSPACE)).toBe(true);
  const afterHold = await inputFrameCount(page);
  await settleNoNewFrames(page, afterHold);
});


test("native Backspace presses produce exact DEL cardinality", async ({ page }) => {
  await openTaskTerminal(page);
  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(3);
  const frames = await terminalInputFrames(page);
  expect(frames.slice(baseline).map((frame) => frame.data)).toEqual([BACKSPACE, BACKSPACE, BACKSPACE]);
});


test("native Backspace after Enter still sends DEL", async ({ page }) => {
  await openTaskTerminal(page);
  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);

  await page.keyboard.press("Backspace");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(2);
  const frames = await terminalInputFrames(page);
  expect(frames.slice(baseline).map((frame) => frame.data)).toEqual(["\r", BACKSPACE]);
});


test("toolbar preserves prior terminal focus for control keys", async ({ page }) => {
  await openTaskTerminal(page);

  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Tab" }).click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await clickTerminalSurfaceInterior(page);

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await terminalToolbar(page).getByRole("button", { name: "Tab" }).click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);
});


test("supported Ctrl toolbar combinations send exact control codes and disarm sticky Ctrl", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const baseline = await inputFrameCount(page);
  const toolbar = terminalToolbar(page);
  const ctrl = toolbar.getByRole("button", { name: "Control modifier" });

  // The dedicated ⌃C toolbar key was removed; Ctrl+C now goes through the Ctrl
  // modifier plus a typed "c", which the last leg of this test covers.
  await ctrl.click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "true");
  await toolbar.getByRole("button", { name: "Left arrow" }).click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await ctrl.click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "true");
  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("c");
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual(["\x1b[1;5D", "\x03"]);
});
test("repeated printable browser events produce exact cardinality", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.press("x");
  await page.keyboard.press("x");
  await page.keyboard.press("x");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(3);
  const frames = await terminalInputFrames(page);
  expect(frames.slice(baseline).map((frame) => frame.data)).toEqual(["x", "x", "x"]);
});
