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
  terminalToolbar,
  terminalInputFrames,
  terminalResizeFrames,
  terminalSocketSummaries,
  openLatestTerminalSocket,
  closeLatestTerminalSocket,
  failLatestTerminalSocket,
  emitLatestTerminalOutput,
  waitForTerminalSocket,
  dispatchViewportEvents,
  syntheticOutwardPinchOnInteractionSurface,
  type ViewportEventKind,
} from "./fixtures";

test.fail(true, "replacement xterm surface lands in the stacked implementation PR");

const OPEN = 1;

async function activeTaskSocketCount(page: import("@playwright/test").Page) {
  const summaries = await terminalSocketSummaries(page);
  return summaries.filter((socket) => socket.readyState === OPEN).length;
}

async function gotoTaskRoute(page: import("@playwright/test").Page) {
  await page.goto("/app.html#/t/web%2Ffix-login");
}

async function clickTerminalSurfaceInterior(page: import("@playwright/test").Page) {
  const surface = terminalSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("terminal surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function inputFrameCount(page: import("@playwright/test").Page) {
  return (await terminalInputFrames(page)).length;
}

type TerminalSize = { cols: number; rows: number };

function hasAdjacentDuplicateSizes(frames: TerminalSize[]): boolean {
  for (let index = 1; index < frames.length; index += 1) {
    const previous = frames[index - 1];
    const current = frames[index];
    if (previous.cols === current.cols && previous.rows === current.rows) return true;
  }
  return false;
}

function sizesEqual(left: TerminalSize | undefined, right: TerminalSize | undefined): boolean {
  return !!left && !!right && left.cols === right.cols && left.rows === right.rows;
}

const VIEWPORT_EVENT_BURST: ViewportEventKind[] = [
  "resize",
  "orientationchange",
  "visualViewport.resize",
  "resize",
  "visualViewport.resize",
  "orientationchange",
  "resize",
  "visualViewport.resize",
];

async function openTaskTerminal(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}

const expandTerminalButton = (page: import("@playwright/test").Page) =>
  terminalSurface(page).getByRole("button", { name: "Expand terminal" });

const newOutputButton = (page: import("@playwright/test").Page) =>
  page.getByRole("button", { name: "New output ↓" });

function scrollbackChunk(from: number, count: number): string {
  let out = "";
  for (let i = from; i < from + count; i += 1) {
    out += `row ${i}\r\n`;
  }
  return out;
}

async function scrollInteractionSurfaceAway(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate((el) => {
    el.scrollTop = Math.max(0, el.scrollTop - 12 * 18);
    el.dispatchEvent(new Event("scroll"));
  });
}

async function clickInteractionSurfaceCenter(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function documentScrollPosition(page: import("@playwright/test").Page) {
  return page.evaluate(() => ({
    windowY: window.scrollY,
    documentY: document.documentElement.scrollTop,
  }));
}

async function syntheticScrollGestureOnInteractionSurface(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  // Playwright dragTo uses synthetic mouse events, not native iOS touch or momentum.
  await surface.dragTo(surface, {
    sourcePosition: { x: box.width / 2, y: box.height * 0.8 },
    targetPosition: { x: box.width / 2, y: box.height * 0.2 },
  });
}

async function longPressInteractionSurface(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  await surface.tap({
    position: { x: box.width / 2, y: box.height / 2 },
    delay: 550,
  });
}

test("task route mounts one terminal surface and opens one socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);

  await waitForTerminalSocket(page);

  const sockets = await terminalSocketSummaries(page);
  expect(sockets).toHaveLength(1);
});

test("delayed socket open shows Connecting then connects", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();

  await openLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();
});

test("socket close reconnects, server error becomes unavailable, and manual reconnect recovers", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const status = page.getByTestId("terminal-status");
  const reconnect = page.getByRole("button", { name: "Reconnect" });

  await closeLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);

  await openLatestTerminalSocket(page);
  await failLatestTerminalSocket(page, "tmux session missing");

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await reconnect.click();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(3);

  await openLatestTerminalSocket(page);

  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("navigation away closes the active socket and removes the surface", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await expect(surface).not.toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(0);
});

const PTY_OUTPUT_CORPUS_CHUNKS: Array<string | number[]> = [
  "ASCII",
  [...new TextEncoder().encode("😀")],
  [...new TextEncoder().encode("e\u0301")],
  [...new TextEncoder().encode("漢")],
  "\x1b[31mRED\x1b[0m\x1b[2K",
  "carriage\rreturn",
  "line\nbreak",
  "crlf\r\nend",
];

test("pty output corpus keeps surface connected without application errors", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});

test("reopening the task route yields one surface and one active socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(surface).not.toBeVisible();

  await gotoTaskRoute(page);

  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

const MULTILINE_UNICODE_CLIPBOARD = "line one\n漢字\ne\u0301";

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
  await toolbar.getByRole("button", { name: "Esc" }).click();
  await toolbar.getByRole("button", { name: "←" }).click();
  await toolbar.getByRole("button", { name: "↑" }).click();
  await toolbar.getByRole("button", { name: "↓" }).click();
  await toolbar.getByRole("button", { name: "→" }).click();

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

test("multiline Unicode paste preserves content in one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});

test("Hide keyboard focus blur adds no PTY input", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await terminalToolbar(page).getByRole("button", { name: "Hide keyboard" }).click();

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

test("typing after manual reconnect sends exactly one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await failLatestTerminalSocket(page, "tmux session missing");

  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();

  await reconnect.click();
  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);
  await openLatestTerminalSocket(page);
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("!");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe("!");
});

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

  await page.setViewportSize({ width: 360, height: 800 });
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

  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty(
      "--app-height",
      `${Math.max(0, window.innerHeight - 336)}px`,
    );
  });
  await page.setViewportSize({ width: 390, height: 508 });

  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);
  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);

  const countAfterKeyboardBurst = (await terminalResizeFrames(page)).length;
  const keyboardOpenFrames = (await terminalResizeFrames(page)).slice(
    countBeforeKeyboard,
    countAfterKeyboardBurst,
  );
  expect(keyboardOpenFrames.length).toBeLessThanOrEqual(1);
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

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
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

test("task route exposes a stable terminal interaction surface locator", async ({ page }) => {
  await openTaskTerminal(page);
  await expect(terminalInteractionSurface(page)).toBeVisible();
});

test("reading scrollback shows New output and restoring live output sends no PTY input", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await expect(newOutputButton(page)).not.toBeVisible();

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

test("supported Ctrl toolbar combinations send exact control codes and disarm sticky Ctrl", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const baseline = await inputFrameCount(page);
  const toolbar = terminalToolbar(page);
  const ctrl = toolbar.getByRole("button", { name: /Ctrl/ });

  await toolbar.getByRole("button", { name: "⌃C" }).click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await ctrl.click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "true");
  await toolbar.getByRole("button", { name: "←" }).click();
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
    .toEqual(["\x03", "\x1b[1;5D", "\x03"]);
});

test("pty output corpus during delayed socket open keeps surface stable without application errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(1);
  const socketsBeforeOpen = await terminalSocketSummaries(page);
  expect(socketsBeforeOpen[0]?.readyState).toBe(0);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await openLatestTerminalSocket(page);

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
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

test("Paste stays available after synthetic scroll gesture and fullscreen transitions", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });
  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 80)]);
  await syntheticScrollGestureOnInteractionSurface(page);

  const expand = expandTerminalButton(page);
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  const paste = terminalToolbar(page).getByRole("button", { name: "Paste" });
  await expect(paste).toBeVisible();

  const baseline = await inputFrameCount(page);
  await paste.click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});
