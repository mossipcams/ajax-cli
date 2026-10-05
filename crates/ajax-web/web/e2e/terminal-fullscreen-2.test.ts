// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.


import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
  terminalInputFrames,
  terminalResizeFrames,
  emitLatestTerminalOutput,
  waitForTerminalSocket
} from "./fixtures";
import {
  BAND_SETTLE_RESIZE_BUDGET,
  activeTaskSocketCount,
  clickInteractionSurfaceCenter,
  documentScrollPosition,
  expandTerminalButton,
  gotoTaskRoute,
  hasAdjacentDuplicateSizes,
  inputFrameCount,
  newOutputButton,
  openKeyboardBandForResizeTests,
  openTaskTerminal,
  scrollInteractionSurfaceAway,
  scrollbackChunk,
  simulateKeyboardBand,
  sizesEqual,
  waitForResizeFrameCountStable,
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


// Backspace is the one key we leave uncancelled (cancelling it kills the iOS
// hold-to-delete repeat), so WebKit really edits the helper textarea and then
// reveals the caret. .terminal-host is position: sticky, so that reveal used to
// yank the wrap — and the whole terminal with it — up into scrollback.
test("terminal Backspace input preserves scroll and keyboard-band geometry", async ({ page }) => {
  await openTaskTerminal(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await scrollInteractionSurfaceAway(page);
  await page.evaluate(() => {
    const ta = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
    if (!(ta instanceof HTMLTextAreaElement)) throw new Error("textarea missing");
    ta.focus({ preventScroll: true });
  });
  await simulateKeyboardBand(page);
  await new Promise((r) => setTimeout(r, 400));

  const read = () =>
    page.evaluate(() => {
      const host = document.querySelector(
        "[data-testid='task-terminal-panel'] .terminal-host",
      ) as (HTMLElement & { __xterm?: { buffer: { active: { viewportY: number } } } }) | null;
      const wrap = document.querySelector(
        "[data-testid='terminal-interaction-surface']",
      ) as HTMLElement | null;
      const panel = document
        .querySelector("[data-testid='task-terminal-panel']")
        ?.getBoundingClientRect();
      const ta = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
      return {
        wrapScrollTop: wrap?.scrollTop ?? -1,
        viewportY: host?.__xterm?.buffer.active.viewportY ?? -1,
        panelTop: panel?.top ?? -1,
        focused: ta === document.activeElement,
        keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      };
    });

  const baseline = await inputFrameCount(page);
  const before = await read();
  expect(before.focused).toBe(true);
  expect(before.keyboardOpen).toBe(true);
  expect(before.wrapScrollTop).toBeGreaterThan(0);

  await page.keyboard.press("Backspace");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe("\x7f");
  await new Promise((r) => setTimeout(r, 400));

  const after = await read();
  expect(after.wrapScrollTop).toBe(before.wrapScrollTop);
  expect(after.viewportY).toBe(before.viewportY);
  expect(Math.abs(after.panelTop - before.panelTop)).toBeLessThanOrEqual(1);
  expect(after.focused).toBe(true);
  expect(after.keyboardOpen).toBe(true);
});


test("keyboard-open expand enters fullscreen with a bounded discreteIntent settle while keyboard stays open", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);
  const countBeforeKeyboard = (await terminalResizeFrames(page)).length;

  await openKeyboardBandForResizeTests(page);

  const keyboardEdgeFrames = (await terminalResizeFrames(page)).slice(countBeforeKeyboard);
  expect(keyboardEdgeFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(keyboardEdgeFrames)).toBe(false);

  const countBeforeExpand = (await terminalResizeFrames(page)).length;
  const expand = expandTerminalButton(page);
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return frames.length > countBeforeExpand && !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await waitForResizeFrameCountStable(page);

  const expandFrames = (await terminalResizeFrames(page)).slice(countBeforeExpand);
  expect(expandFrames.length).toBeGreaterThanOrEqual(1);
  expect(expandFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(expandFrames)).toBe(false);
  const expandFrame = expandFrames.at(-1)!;
  expect(sizesEqual(expandFrame, settledBefore!)).toBe(false);
  expect(expandFrame.cols).toBeGreaterThan(0);
  expect(expandFrame.rows).toBeGreaterThan(0);
  expect(Number.isInteger(expandFrame.cols)).toBe(true);
  expect(Number.isInteger(expandFrame.rows)).toBe(true);
  expect(
    await page.evaluate(() => document.documentElement.classList.contains("keyboard-open")),
  ).toBe(true);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("desktop expanded mode keeps terminal bounded and task details summary reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

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

  const summary = page.locator(".meta-details summary");
  await summary.scrollIntoViewIfNeeded();
  await expect(summary).toBeInViewport();
});


test("New output click does not refocus xterm or reopen keyboard, and direct surface click focuses without scrolling", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const isTermFocused = () =>
    page.evaluate(() => {
      const textarea = document.querySelector(
        ".terminal-host textarea.xterm-helper-textarea",
      );
      return textarea === document.activeElement;
    });
  const isKeyboardOpen = () =>
    page.evaluate(() => document.documentElement.classList.contains("keyboard-open"));

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await waitForSeedRevealSettled(page);
  await scrollInteractionSurfaceAway(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(200, 40)]);

  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();

  expect(await isTermFocused()).toBe(false);
  expect(await isKeyboardOpen()).toBe(false);

  await newOutput.click();

  expect(await isTermFocused()).toBe(false);
  expect(await isKeyboardOpen()).toBe(false);
  await expect(newOutput).not.toBeVisible();

  const scrollBefore = await documentScrollPosition(page);
  await clickInteractionSurfaceCenter(page);
  const scrollAfter = await documentScrollPosition(page);

  expect(scrollAfter).toEqual(scrollBefore);
  await expect
    .poll(async () => isTermFocused())
    .toBe(true);
});


test("fullscreen enter and exit keep one socket, one surface, and ordered PTY input", async ({
  page,
}) => {
  const surface = await openTaskTerminal(page);
  const expand = expandTerminalButton(page);
  const baseline = await inputFrameCount(page);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("1");

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("2");

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual(["1", "2"]);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});
