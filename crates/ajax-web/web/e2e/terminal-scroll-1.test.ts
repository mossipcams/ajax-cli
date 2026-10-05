// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.


import { test, expect } from "@playwright/test";
import {
  terminalInteractionSurface,
  terminalInputFrames,
  emitLatestTerminalOutput
} from "./fixtures";
import {
  documentScrollPosition,
  inputFrameCount,
  newOutputButton,
  openTaskTerminal,
  scrollInteractionSurfaceAway,
  scrollbackChunk,
  syntheticScrollGestureOnInteractionSurface,
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


async function longPressInteractionSurface(
  page: import("@playwright/test").Page,
  position?: { x: number; y: number },
) {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate(
    async (el, pos: { x: number; y: number } | null) => {
      const rect = el.getBoundingClientRect();
      const clientX = rect.left + (pos?.x ?? rect.width / 2);
      const clientY = rect.top + (pos?.y ?? rect.height / 2);
      const touch = { clientX, clientY, identifier: 0, target: el };
      const makeTouch = (type: string, touches: typeof touch[]) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, "touches", { value: touches });
        Object.defineProperty(event, "changedTouches", { value: touches });
        return event;
      };
      el.dispatchEvent(makeTouch("touchstart", [touch]));
      // Hold past LONG_PRESS_MS with headroom for CI timer delay.
      await new Promise((resolve) => setTimeout(resolve, 750));
      el.dispatchEvent(makeTouch("touchend", []));
    },
    position ?? null,
  );
}


test("task route exposes a stable terminal interaction surface locator", async ({ page }) => {
  await openTaskTerminal(page);
  await expect(terminalInteractionSurface(page)).toBeVisible();
});


// Scrollback's *content* half. The sibling test below asserts only the New
// output badge, which refreshFollow drives — it passes even if the terminal
// viewport never moves, so nothing here covered the content actually scrolling.
//
// Caveat, recorded honestly: this does NOT reproduce the slice 10 round 2a
// scrollback regression. It drives scroll by assigning scrollTop, which is not
// how a user scrolls; a real touch drag goes through the browser's native
// scroll pipeline, and synthetic touch events do not trigger that. Reproducing
// that regression needs the iOS simulator or a device. This test is still worth
// keeping — it closes a real assertion gap — but it is not the guard that would
// have caught round 2a.
test("scrolling the interaction wrapper moves the terminal viewport", async ({ page }) => {
  await openTaskTerminal(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);

  const viewportY = () =>
    page.evaluate(() => {
      const host = document.querySelector(
        "[data-testid='task-terminal-panel'] .terminal-host",
      ) as (HTMLElement & { __xterm?: { buffer: { active: { viewportY: number } } } }) | null;
      return host?.__xterm?.buffer.active.viewportY ?? -1;
    });

  await waitForSeedRevealSettled(page);

  await expect.poll(async () => viewportY()).toBeGreaterThan(0);
  const atBottom = await viewportY();

  await scrollInteractionSurfaceAway(page);

  // The wrapper drove the scroll, so syncTermFromWrapper must have moved the
  // terminal's own viewport up. Without this the user sees the New output badge
  // appear while the text stays put.
  await expect.poll(async () => await viewportY()).toBeLessThan(atBottom);
});


test("reading scrollback shows New output and restoring live output sends no PTY input", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await expect(newOutputButton(page)).not.toBeVisible();

  await waitForSeedRevealSettled(page);
  await scrollInteractionSurfaceAway(page);

  const baseline = await inputFrameCount(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(200, 40)]);

  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();
  await newOutput.click();
  await expect(newOutput).not.toBeVisible();
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});


test("long press on the interaction surface sends no PTY input", async ({ page }) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, ["selectable terminal text\r\n"]);
  const baseline = await inputFrameCount(page);

  await longPressInteractionSurface(page);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});


test("held terminal directional drag repeats its locked arrow without stealing normal scroll", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const surface = terminalInteractionSurface(page);
  const touchAction = await surface.evaluate((el) => getComputedStyle(el).touchAction);
  expect(touchAction).toBe("pan-y");

  const baselineEarly = await inputFrameCount(page);
  const earlyMove = await surface.evaluate(async (el) => {
    const rect = el.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const makeTouch = (clientX: number, clientY: number) => ({
      clientX,
      clientY,
      identifier: 0,
      target: el,
    });
    const dispatch = (type: string, touches: ReturnType<typeof makeTouch>[]) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", { value: touches });
      Object.defineProperty(event, "changedTouches", { value: touches });
      el.dispatchEvent(event);
      return event.defaultPrevented;
    };
    dispatch("touchstart", [makeTouch(x, y)]);
    const cancelled = dispatch("touchmove", [makeTouch(x, y + 40)]);
    dispatch("touchend", []);
    return cancelled;
  });
  expect(earlyMove).toBe(false);
  await expect.poll(async () => inputFrameCount(page)).toBe(baselineEarly);

  const directions: Array<{
    name: string;
    dx: number;
    dy: number;
    arrow: string;
    endWith: "touchend" | "touchcancel";
  }> = [
    { name: "up", dx: 0, dy: -40, arrow: "\x1b[A", endWith: "touchend" },
    { name: "right", dx: 40, dy: 0, arrow: "\x1b[C", endWith: "touchend" },
    { name: "down", dx: 0, dy: 40, arrow: "\x1b[B", endWith: "touchend" },
    { name: "left", dx: -40, dy: 0, arrow: "\x1b[D", endWith: "touchcancel" },
  ];

  for (const dir of directions) {
    const baseline = await inputFrameCount(page);
    const cancelled = await surface.evaluate(
      async (el, spec: { dx: number; dy: number; endWith: string }) => {
        const rect = el.getBoundingClientRect();
        const x = rect.left + rect.width / 2;
        const y = rect.top + rect.height / 2;
        const makeTouch = (clientX: number, clientY: number) => ({
          clientX,
          clientY,
          identifier: 0,
          target: el,
        });
        const dispatch = (type: string, touches: ReturnType<typeof makeTouch>[]) => {
          const event = new Event(type, { bubbles: true, cancelable: true });
          Object.defineProperty(event, "touches", { value: touches });
          Object.defineProperty(event, "changedTouches", { value: touches });
          el.dispatchEvent(event);
          return event.defaultPrevented;
        };
        dispatch("touchstart", [makeTouch(x, y)]);
        // Hold past LONG_PRESS_MS with headroom for CI timer delay.
        await new Promise((resolve) => setTimeout(resolve, 750));
        const moveCancelled = dispatch("touchmove", [makeTouch(x + spec.dx, y + spec.dy)]);
        // Allow short repeat cadence to emit at least one more frame.
        await new Promise((resolve) => setTimeout(resolve, 200));
        dispatch(spec.endWith, []);
        return moveCancelled;
      },
      { dx: dir.dx, dy: dir.dy, endWith: dir.endWith },
    );
    expect(cancelled, `${dir.name} move should be cancelled`).toBe(true);

    await expect
      .poll(async () => (await inputFrameCount(page)) - baseline)
      .toBeGreaterThanOrEqual(2);
    const heldFrames = (await terminalInputFrames(page)).slice(baseline);
    expect(
      heldFrames.every((frame) => frame.data === dir.arrow),
      `${dir.name} frames must be only ${JSON.stringify(dir.arrow)}`,
    ).toBe(true);
    const heldCount = heldFrames.length;

    await new Promise((resolve) => setTimeout(resolve, 200));
    await expect.poll(async () => inputFrameCount(page)).toBe(baseline + heldCount);
  }

  const baselineNativeScroll = await inputFrameCount(page);
  const nativeScrollMove = await surface.evaluate(async (el) => {
    const rect = el.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const makeTouch = (clientX: number, clientY: number) => ({
      clientX,
      clientY,
      identifier: 0,
      target: el,
    });
    const dispatch = (type: string, touches: ReturnType<typeof makeTouch>[], cancelable: boolean) => {
      const event = new Event(type, { bubbles: true, cancelable });
      Object.defineProperty(event, "touches", { value: touches });
      Object.defineProperty(event, "changedTouches", { value: touches });
      el.dispatchEvent(event);
      return event.defaultPrevented;
    };
    dispatch("touchstart", [makeTouch(x, y)], true);
    // Hold past LONG_PRESS_MS with headroom for CI timer delay.
    await new Promise((resolve) => setTimeout(resolve, 750));
    // Native pan-y scroll can deliver a non-cancelable vertical move.
    const cancelled = dispatch("touchmove", [makeTouch(x, y + 40)], false);
    await new Promise((resolve) => setTimeout(resolve, 200));
    dispatch("touchend", [], true);
    return cancelled;
  });
  expect(nativeScrollMove).toBe(false);
  await expect.poll(async () => inputFrameCount(page)).toBe(baselineNativeScroll);

  const baselineHandoff = await inputFrameCount(page);
  const handoff = await surface.evaluate(async (el) => {
    const rect = el.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const makeTouch = (clientX: number, clientY: number) => ({
      clientX,
      clientY,
      identifier: 0,
      target: el,
    });
    const dispatch = (type: string, touches: ReturnType<typeof makeTouch>[], cancelable: boolean) => {
      const event = new Event(type, { bubbles: true, cancelable });
      Object.defineProperty(event, "touches", { value: touches });
      Object.defineProperty(event, "changedTouches", { value: touches });
      el.dispatchEvent(event);
      return event.defaultPrevented;
    };
    dispatch("touchstart", [makeTouch(x, y)], true);
    await new Promise((resolve) => setTimeout(resolve, 750));
    const armedCancelled = dispatch("touchmove", [makeTouch(x, y - 40)], true);
    await new Promise((resolve) => setTimeout(resolve, 200));
    const afterArmCount = (
      (window as unknown as { __terminalFrames?: Array<{ type?: string }> }).__terminalFrames ?? []
    ).filter((frame) => frame?.type === "input").length;
    const laterCancelled = dispatch("touchmove", [makeTouch(x, y - 80)], false);
    await new Promise((resolve) => setTimeout(resolve, 200));
    const afterHandoffCount = (
      (window as unknown as { __terminalFrames?: Array<{ type?: string }> }).__terminalFrames ?? []
    ).filter((frame) => frame?.type === "input").length;
    dispatch("touchend", [], true);
    return { armedCancelled, laterCancelled, afterArmCount, afterHandoffCount };
  });
  expect(handoff.armedCancelled).toBe(true);
  expect(handoff.afterArmCount).toBeGreaterThan(baselineHandoff);
  expect(handoff.laterCancelled).toBe(false);
  expect(handoff.afterHandoffCount).toBe(handoff.afterArmCount);
  await new Promise((resolve) => setTimeout(resolve, 200));
  await expect.poll(async () => inputFrameCount(page)).toBe(handoff.afterHandoffCount);

  expect(await surface.evaluate((el) => getComputedStyle(el).touchAction)).toBe("pan-y");
});


test("synthetic scroll gesture on the interaction surface sends no PTY input and does not move the document", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 120)]);
  const scrollBefore = await documentScrollPosition(page);
  const baseline = await inputFrameCount(page);

  await syntheticScrollGestureOnInteractionSurface(page);

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  const scrollAfter = await documentScrollPosition(page);
  expect(scrollAfter).toEqual(scrollBefore);
});
