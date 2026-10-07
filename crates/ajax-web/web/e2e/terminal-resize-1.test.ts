import { test, expect } from "@playwright/test";
import {
  terminalSurface,
  terminalInteractionSurface,
  terminalInputFrames,
  terminalResizeFrames,
  emitLatestTerminalOutput,
  waitForTerminalSocket
} from "./fixtures";
import { dispatchViewportEvents } from "./fixtures";
import { syntheticOutwardPinchOnInteractionSurface } from "./fixtures";
import {
  BAND_SETTLE_RESIZE_BUDGET,
  PTY_OUTPUT_CORPUS_CHUNKS,
  VIEWPORT_EVENT_BURST,
  activeTaskSocketCount,
  clickInteractionSurfaceCenter,
  expandTerminalButton,
  gotoTaskRoute,
  hasAdjacentDuplicateSizes,
  inputFrameCount,
  openKeyboardBandForResizeTests,
  openTaskTerminal,
  sizesEqual,
  waitForResizeFrameCountStable
} from "./terminal-behavior-helpers";


test.beforeEach((_, testInfo) => {
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


async function readLogicalXtermGeometry(page: import("@playwright/test").Page) {
  return page.locator("[data-testid='task-terminal-panel'] .terminal-host .xterm").evaluate((xtermEl) => {
    const host = xtermEl.parentElement as HTMLElement | null;
    const screen = xtermEl.querySelector(".xterm-screen") as HTMLElement | null;
    if (!host || !screen) throw new Error("terminal host or xterm screen missing");
    const rendered = xtermEl.getBoundingClientRect();
    return {
      hostWidth: host.clientWidth,
      hostHeight: host.clientHeight,
      logicalWidth: xtermEl.offsetWidth,
      logicalHeight: xtermEl.offsetHeight,
      screenWidth: screen.offsetWidth,
      screenHeight: screen.offsetHeight,
      renderedWidth: rendered.width,
      renderedHeight: rendered.height,
    };
  });
}


test("initial open eventually sends at least one valid positive-integer PTY size", async ({ page }) => {
  await openTaskTerminal(page);

  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);
  const frames = await terminalResizeFrames(page);
  for (const frame of frames) {
    expect(frame.cols).toBeGreaterThan(0);
    expect(frame.rows).toBeGreaterThan(0);
    expect(Number.isInteger(frame.cols)).toBe(true);
    expect(Number.isInteger(frame.rows)).toBe(true);
  }
});


test("logical xterm grid is at least 80 columns and scales to fill the phone host", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const geometry = await readLogicalXtermGeometry(page);

  expect(geometry.screenWidth).toBeGreaterThan(geometry.hostWidth);
  expect(geometry.screenHeight).toBeGreaterThan(geometry.hostHeight);
  expect(geometry.logicalWidth).toBeGreaterThan(geometry.hostWidth);
  expect(geometry.logicalHeight).toBeGreaterThan(geometry.hostHeight);

  expect(geometry.renderedWidth).toBeGreaterThanOrEqual(geometry.hostWidth - 2);
  expect(geometry.renderedWidth).toBeLessThanOrEqual(geometry.hostWidth + 2);
  expect(geometry.renderedHeight).toBeGreaterThanOrEqual(geometry.hostHeight - 2);
  expect(geometry.renderedHeight).toBeLessThanOrEqual(geometry.hostHeight + 2);

  const lastResize = (await terminalResizeFrames(page)).at(-1)!;
  expect(lastResize.cols).toBeGreaterThanOrEqual(80);
});


test("portrait-to-landscape eventually produces a fresh valid resize without adjacent duplicate sizes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const beforeLast = (await terminalResizeFrames(page)).at(-1);
  const sliceStart = (await terminalResizeFrames(page)).length;

  await page.setViewportSize({ width: 844, height: 390 });
  await dispatchViewportEvents(page, ["orientationchange", "resize", "visualViewport.resize"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, beforeLast);
    })
    .toBe(true);

  const transitionFrames = (await terminalResizeFrames(page)).slice(sliceStart);
  expect(transitionFrames.length).toBeGreaterThan(0);
  expect(hasAdjacentDuplicateSizes(transitionFrames)).toBe(false);
});


test("repeated same-dimension viewport burst then meaningful change deduplicates resize outcomes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);
  const countBeforeBurst = (await terminalResizeFrames(page)).length;

  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);
  const countAfterBurst = (await terminalResizeFrames(page)).length;
  expect(countAfterBurst).toBe(countBeforeBurst);

  await page.setViewportSize({ width: 360, height: 640 });
  await dispatchViewportEvents(page, ["resize", "visualViewport.resize"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  const transitionFrames = (await terminalResizeFrames(page)).slice(countBeforeBurst);
  expect(hasAdjacentDuplicateSizes(transitionFrames)).toBe(false);
});


test("keyboard-open resize burst does not storm PTY resize; closing eventually settles without adjacent duplicates", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const countBeforeKeyboard = (await terminalResizeFrames(page)).length;

  await openKeyboardBandForResizeTests(page);
  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);

  const countAfterKeyboardBurst = (await terminalResizeFrames(page)).length;
  const keyboardOpenFrames = (await terminalResizeFrames(page)).slice(
    countBeforeKeyboard,
    countAfterKeyboardBurst,
  );
  expect(keyboardOpenFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(keyboardOpenFrames)).toBe(false);

  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
  });
  await page.setViewportSize({ width: 390, height: 800 });
  await dispatchViewportEvents(page, ["visualViewport.resize", "resize", "orientationchange"]);

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countAfterKeyboardBurst);

  const afterCloseFrames = (await terminalResizeFrames(page)).slice(countAfterKeyboardBurst);
  expect(afterCloseFrames.length).toBeGreaterThan(0);
  expect(hasAdjacentDuplicateSizes(afterCloseFrames)).toBe(false);
});


test("keyboard-open pinch-end produces a bounded fresh PTY resize while keyboard stays open", async ({
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

  const countBeforePinch = (await terminalResizeFrames(page)).length;

  await syntheticOutwardPinchOnInteractionSurface(page);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return frames.length > countBeforePinch && !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await waitForResizeFrameCountStable(page);

  const pinchFrames = (await terminalResizeFrames(page)).slice(countBeforePinch);
  expect(pinchFrames.length).toBeGreaterThanOrEqual(1);
  expect(pinchFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(pinchFrames)).toBe(false);
  const pinchFrame = pinchFrames.at(-1)!;
  expect(sizesEqual(pinchFrame, settledBefore!)).toBe(false);
  expect(pinchFrame.cols).toBeGreaterThan(0);
  expect(pinchFrame.rows).toBeGreaterThan(0);
  expect(Number.isInteger(pinchFrame.cols)).toBe(true);
  expect(Number.isInteger(pinchFrame.rows)).toBe(true);
  expect(
    await page.evaluate(() => document.documentElement.classList.contains("keyboard-open")),
  ).toBe(true);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("fullscreen enter and exit each produce a fresh valid resize and retain one active socket", async ({
  page,
}) => {
  const surface = await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const expand = expandTerminalButton(page);
  const countBeforeExpand = (await terminalResizeFrames(page)).length;

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countBeforeExpand);

  const expandedLast = (await terminalResizeFrames(page)).at(-1)!;
  expect(expandedLast.cols).toBeGreaterThan(0);
  expect(expandedLast.rows).toBeGreaterThan(0);

  const countAfterExpand = (await terminalResizeFrames(page)).length;
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countAfterExpand);

  const exitFrames = (await terminalResizeFrames(page)).slice(countAfterExpand);
  expect(hasAdjacentDuplicateSizes(exitFrames)).toBe(false);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("reopen with meaningful viewport change yields one surface and deduplicated resize outcomes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const framesBeforeNav = (await terminalResizeFrames(page)).length;

  await page.evaluate(() => {
    window.location.hash = "#/";
  });
  await expect(terminalSurface(page)).not.toBeVisible();

  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(framesBeforeNav);

  const settledBeforeChange = (await terminalResizeFrames(page)).at(-1);
  const sliceStart = (await terminalResizeFrames(page)).length;

  await page.setViewportSize({ width: 375, height: 812 });
  await dispatchViewportEvents(page, ["resize", "visualViewport.resize", "orientationchange"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBeforeChange);
    })
    .toBe(true);

  const changeFrames = (await terminalResizeFrames(page)).slice(sliceStart);
  expect(hasAdjacentDuplicateSizes(changeFrames)).toBe(false);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("outward pinch on the interaction surface changes PTY size and persists across reload", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBeforePinch = (await terminalResizeFrames(page)).at(-1);
  const resizeSliceStart = (await terminalResizeFrames(page)).length;
  const inputBaseline = await inputFrameCount(page);

  await syntheticOutwardPinchOnInteractionSurface(page);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBeforePinch);
    })
    .toBe(true);

  const settledAfterPinch = (await terminalResizeFrames(page)).at(-1)!;
  const pinchFrames = (await terminalResizeFrames(page)).slice(resizeSliceStart);
  expect(hasAdjacentDuplicateSizes(pinchFrames)).toBe(false);
  expect(!sizesEqual(settledAfterPinch, settledBeforePinch)).toBe(true);

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("p");

  await expect.poll(async () => (await inputFrameCount(page)) - inputBaseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe("p");

  await page.reload();

  await expect(terminalSurface(page)).toBeVisible({ timeout: 10_000 });
  await expect(terminalInteractionSurface(page)).toBeVisible();
  await waitForTerminalSocket(page);

  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);
  const settledAfterReload = (await terminalResizeFrames(page)).at(-1)!;
  expect(!sizesEqual(settledAfterReload, settledBeforePinch)).toBe(true);

  const reloadInputBaseline = await inputFrameCount(page);
  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("q");

  await expect.poll(async () => (await inputFrameCount(page)) - reloadInputBaseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe("q");
});


test("rapid pty output during viewport transition eventually settles resize without application errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  const surface = await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);

  await page.setViewportSize({ width: 844, height: 390 });
  await dispatchViewportEvents(page, ["orientationchange", "resize", "visualViewport.resize"]);
  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);
  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});
test("scheduled terminal work does not survive disposal after immediate navigation away", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  await expandTerminalButton(page).click();
  await page.goto("/app.html#/");

  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => resolve());
        });
      }),
  );

  expect(pageErrors).toEqual([]);
  await expect(terminalSurface(page)).not.toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(0);
});
