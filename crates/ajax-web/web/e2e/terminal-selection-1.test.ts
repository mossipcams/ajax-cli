// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.


import { test, expect } from "@playwright/test";
import {
  terminalInteractionSurface,
  emitLatestTerminalOutput
} from "./fixtures";
import {
  COPY_SELECTION_TEXT,
  inputFrameCount,
  openTaskTerminalWithCopyFailure,
  openTaskTerminalWithCopySpy,
  programTerminalSelection,
  terminalPanel,
  waitForStableCopyButton
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
/** Press the center of the first cell of `needle` using live xterm screen metrics. */
async function longPressTerminalText(
  page: import("@playwright/test").Page,
  needle: string,
) {
  await expect
    .poll(async () =>
      page.evaluate((text) => {
        const host = document.querySelector(
          "[data-testid='task-terminal-panel'] .terminal-host",
        ) as (HTMLElement & {
          __xterm?: {
            buffer: {
              active: {
                length: number;
                getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined;
              };
            };
          };
        }) | null;
        const term = host?.__xterm;
        if (!term) return false;
        for (let row = 0; row < term.buffer.active.length; row += 1) {
          const line = term.buffer.active.getLine(row);
          if (line?.translateToString(true).includes(text)) return true;
        }
        return false;
      }, needle),
    )
    .toBe(true);

  const pos = await page.evaluate((text) => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as (HTMLElement & {
      __xterm?: {
        cols: number;
        rows: number;
        element: HTMLElement | undefined;
        buffer: {
          active: {
            viewportY: number;
            length: number;
            getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined;
          };
        };
      };
    }) | null;
    const term = host?.__xterm;
    const surfaceEl = document.querySelector(
      "[data-testid='terminal-interaction-surface']",
    ) as HTMLElement | null;
    if (!term?.element || !surfaceEl || term.cols <= 0 || term.rows <= 0) {
      throw new Error("terminal metrics missing for long-press");
    }
    let bufferRow = -1;
    let col = -1;
    for (let row = 0; row < term.buffer.active.length; row += 1) {
      const line = term.buffer.active.getLine(row);
      if (!line) continue;
      const idx = line.translateToString(true).indexOf(text);
      if (idx >= 0) {
        bufferRow = row;
        col = idx;
        break;
      }
    }
    if (bufferRow < 0 || col < 0) throw new Error(`text not in buffer: ${text}`);

    const screenEl = term.element.querySelector(".xterm-screen") as HTMLElement | null;
    const bounds = (screenEl ?? host!).getBoundingClientRect();
    const surfaceRect = surfaceEl.getBoundingClientRect();
    const cellWidth = bounds.width / term.cols;
    const cellHeight = bounds.height / term.rows;
    const rowInView = bufferRow - term.buffer.active.viewportY;
    const clientX = bounds.left + (col + 0.5) * cellWidth;
    const clientY = bounds.top + (rowInView + 0.5) * cellHeight;
    return {
      x: clientX - surfaceRect.left,
      y: clientY - surfaceRect.top,
    };
  }, needle);

  await longPressInteractionSurface(page, pos);
}


const clipboardWrites = (page: import("@playwright/test").Page) =>
  page.evaluate(() => (window as unknown as { __clipboardWrites: string[] }).__clipboardWrites);


test("long press on known output text selects word and shows Copy control", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  const baseline = await inputFrameCount(page);

  await longPressTerminalText(page, COPY_SELECTION_TEXT);

  await expect(terminalPanel(page).getByRole("button", { name: "Copy" })).toBeVisible({
    timeout: 10_000,
  });
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});


test("non-empty xterm selection shows Copy control in terminal panel", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  await expect(terminalPanel(page).getByRole("button", { name: "Copy" })).toBeVisible();
});


test("selection Copy sits beside the fullscreen expand control", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByTestId("terminal-copy-overlay");
  const expand = terminalPanel(page).locator(".terminal-expand-corner");
  await expect(copy).toBeVisible();
  await expect(expand).toBeVisible();

  const geometry = await page.evaluate(() => {
    const copyEl = document.querySelector<HTMLElement>('[data-testid="terminal-copy-overlay"]');
    const expandEl = document.querySelector<HTMLElement>(".terminal-expand-corner");
    const wrap = document.querySelector<HTMLElement>(
      '[data-testid="terminal-interaction-surface"]',
    );
    const corner = document.querySelector<HTMLElement>(".terminal-corner-actions");
    const panel = document.querySelector<HTMLElement>('[data-testid="task-terminal-panel"]');
    if (!copyEl || !expandEl || !wrap || !corner || !panel) return null;
    const copyBox = copyEl.getBoundingClientRect();
    const expandBox = expandEl.getBoundingClientRect();
    const panelBox = panel.getBoundingClientRect();
    return {
      sharedParent: copyEl.parentElement === corner && expandEl.parentElement === corner,
      copyInsideScrollWrap: wrap.contains(copyEl),
      expandInsideScrollWrap: wrap.contains(expandEl),
      copyRight: copyBox.right,
      copyTop: copyBox.top,
      copyBottom: copyBox.bottom,
      copyWidth: copyBox.width,
      copyHeight: copyBox.height,
      expandLeft: expandBox.left,
      expandTop: expandBox.top,
      expandRight: expandBox.right,
      expandHeight: expandBox.height,
      panelTop: panelBox.top,
      panelRight: panelBox.right,
      gap: expandBox.left - copyBox.right,
    };
  });

  expect(geometry).not.toBeNull();
  expect(geometry!.sharedParent).toBe(true);
  expect(geometry!.copyInsideScrollWrap).toBe(false);
  expect(geometry!.expandInsideScrollWrap).toBe(false);
  expect(geometry!.copyRight).toBeLessThanOrEqual(geometry!.expandLeft + 1);
  expect(geometry!.gap).toBeGreaterThanOrEqual(0);
  expect(geometry!.gap).toBeLessThanOrEqual(12);
  expect(Math.abs(geometry!.copyTop - geometry!.expandTop)).toBeLessThanOrEqual(4);
  expect(geometry!.copyTop).toBeGreaterThanOrEqual(geometry!.panelTop);
  expect(geometry!.copyTop).toBeLessThanOrEqual(geometry!.panelTop + 16);
  expect(geometry!.expandRight).toBeLessThanOrEqual(geometry!.panelRight + 1);
  expect(geometry!.copyWidth).toBeGreaterThanOrEqual(44);
  expect(geometry!.copyHeight).toBeGreaterThanOrEqual(44);
  expect(geometry!.expandHeight).toBeGreaterThanOrEqual(44);
});


test("selection Copy stays pinned beside expand after scrolling the interaction wrap", async ({
  page,
}) => {
  await openTaskTerminalWithCopySpy(page);
  await emitLatestTerminalOutput(page, [
    `${COPY_SELECTION_TEXT}\r\n`,
    ...Array.from({ length: 80 }, (_, i) => `scroll-line-${i}\r\n`),
  ]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByTestId("terminal-copy-overlay");
  await expect(copy).toBeVisible();
  await expect
    .poll(async () => (await copy.boundingBox())?.height ?? 0, { timeout: 5_000 })
    .toBeGreaterThan(0);

  const before = await copy.boundingBox();
  expect(before).not.toBeNull();

  await terminalInteractionSurface(page).evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });

  const after = await page.evaluate(() => {
    const copyEl = document.querySelector<HTMLElement>('[data-testid="terminal-copy-overlay"]');
    const expandEl = document.querySelector<HTMLElement>(".terminal-expand-corner");
    const wrap = document.querySelector<HTMLElement>(
      '[data-testid="terminal-interaction-surface"]',
    );
    if (!copyEl || !expandEl || !wrap) return null;
    const copyBox = copyEl.getBoundingClientRect();
    const expandBox = expandEl.getBoundingClientRect();
    return {
      scrollTop: wrap.scrollTop,
      copyTop: copyBox.top,
      copyRight: copyBox.right,
      expandLeft: expandBox.left,
      expandTop: expandBox.top,
      visible: getComputedStyle(copyEl).display !== "none" && copyBox.height > 0,
    };
  });

  expect(after).not.toBeNull();
  expect(after!.scrollTop).toBeGreaterThan(0);
  expect(after!.visible).toBe(true);
  expect(after!.copyTop).toBeCloseTo(before!.y, 0);
  expect(after!.copyRight).toBeLessThanOrEqual(after!.expandLeft + 1);
  expect(Math.abs(after!.copyTop - after!.expandTop)).toBeLessThanOrEqual(4);
});


test("Copy writes selected text to clipboard and shows Copied notice", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = await waitForStableCopyButton(page);
  await copy.click();

  await expect.poll(() => clipboardWrites(page)).toContain(COPY_SELECTION_TEXT);
  await expect(terminalPanel(page).getByRole("status")).toContainText("Copied");
  await expect(copy).not.toBeVisible();
});


test("Copy opens read-only fallback when clipboard write fails", async ({ page }) => {
  await openTaskTerminalWithCopyFailure(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = await waitForStableCopyButton(page);
  await copy.click();

  const fallback = page.getByRole("textbox", { name: "Copy text" });
  await expect(fallback).toBeVisible();
  await expect(fallback).toHaveAttribute("readonly", "");
  await expect(fallback).toHaveValue(COPY_SELECTION_TEXT);
});
