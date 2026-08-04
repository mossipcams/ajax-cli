// Double-tap-hold-drag terminal copy selection. Kept separate from
// terminal-behavior.test.ts so this PR does not touch that already-over-LOC file.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
  terminalInputFrames,
  emitLatestTerminalOutput,
  waitForTerminalSocket,
} from "./fixtures";

const DOUBLE_TAP_DRAG_START = "dragstartword";
const DOUBLE_TAP_DRAG_END = "dragendword";

const terminalPanel = (page: import("@playwright/test").Page) =>
  page.getByTestId("task-terminal-panel");

async function inputFrameCount(page: import("@playwright/test").Page) {
  return (await terminalInputFrames(page)).length;
}

async function gotoTaskRoute(page: import("@playwright/test").Page) {
  await page.goto("/app.html#/t/web%2Ffix-login");
}

async function installCopyClipboardSpy(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    const writes: string[] = [];
    Object.defineProperty(window, "__clipboardWrites", { value: writes, configurable: true });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          writes.push(text);
        },
        readText: async () => "echo pasted",
      },
    });
  });
}

async function openTaskTerminalWithCopySpy(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await installCopyClipboardSpy(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}

/** Double-tap hold-drag: measure coords, first tap, re-measure, second tap+drag in one turn. */
async function doubleTapHoldDragTerminalText(
  page: import("@playwright/test").Page,
  startNeedle: string,
  endNeedle: string,
) {
  await expect
    .poll(async () =>
      page.evaluate(
        (args) => {
          const { startNeedle: startText, endNeedle: endText } = args;
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
            const str = line?.translateToString(true) ?? "";
            if (str.includes(startText) && str.includes(endText)) return true;
          }
          return false;
        },
        { startNeedle, endNeedle },
      ),
    )
    .toBe(true);

  const surface = terminalInteractionSurface(page);
  await surface.evaluate(
    async (el, args) => {
      const { startNeedle: startText, endNeedle: endText } = args as {
        startNeedle: string;
        endNeedle: string;
      };

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
      if (!term?.element || term.cols <= 0 || term.rows <= 0) {
        throw new Error("terminal metrics missing for double-tap drag");
      }

      const findNeedleCols = () => {
        for (let row = 0; row < term.buffer.active.length; row += 1) {
          const line = term.buffer.active.getLine(row);
          if (!line) continue;
          const str = line.translateToString(true);
          const startIdx = str.indexOf(startText);
          const endIdx = str.indexOf(endText);
          if (startIdx >= 0 && endIdx >= 0) {
            return {
              bufferRow: row,
              startCol: startIdx + Math.floor(startText.length / 2),
              endDragCol: endIdx + endText.length - 1,
            };
          }
        }
        throw new Error(`drag needles not on one line: ${startText} / ${endText}`);
      };

      const clientCoordsForCol = (bufferRow: number, col: number) => {
        const surfaceRect = el.getBoundingClientRect();
        const screenEl = term.element!.querySelector(".xterm-screen") as HTMLElement | null;
        const bounds = (screenEl ?? host!).getBoundingClientRect();
        const cellWidth = bounds.width / term.cols;
        const cellHeight = bounds.height / term.rows;
        const rowInView = bufferRow - term.buffer.active.viewportY;
        const clientX = bounds.left + (col + 0.5) * cellWidth;
        const clientY = bounds.top + (rowInView + 0.5) * cellHeight;
        return {
          x: clientX - surfaceRect.left,
          y: clientY - surfaceRect.top,
        };
      };

      const makeTouch = (type: string, touches: { clientX: number; clientY: number }[]) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, "touches", { value: touches });
        Object.defineProperty(event, "changedTouches", { value: touches });
        return event;
      };
      const touchAt = (surfaceX: number, surfaceY: number) => {
        const surfaceRect = el.getBoundingClientRect();
        return {
          clientX: surfaceRect.left + surfaceX,
          clientY: surfaceRect.top + surfaceY,
          identifier: 0,
          target: el,
        };
      };

      const initial = findNeedleCols();
      const focusCoords = clientCoordsForCol(initial.bufferRow, initial.startCol);

      const focusTouch = touchAt(focusCoords.x, focusCoords.y);
      el.dispatchEvent(makeTouch("touchstart", [focusTouch]));
      await new Promise((resolve) => setTimeout(resolve, 80));
      el.dispatchEvent(makeTouch("touchend", []));
      await new Promise((resolve) => setTimeout(resolve, 200));
      (
        document.querySelector(
          ".terminal-host .xterm-helper-textarea",
        ) as HTMLTextAreaElement | null
      )?.blur();
      await new Promise((resolve) => setTimeout(resolve, 200));

      const fresh = findNeedleCols();
      const startCoords = clientCoordsForCol(fresh.bufferRow, fresh.startCol);

      const first = touchAt(startCoords.x, startCoords.y);
      el.dispatchEvent(makeTouch("touchstart", [first]));
      await new Promise((resolve) => setTimeout(resolve, 80));
      el.dispatchEvent(makeTouch("touchend", []));
      await new Promise((resolve) => setTimeout(resolve, 100));

      const second = touchAt(startCoords.x, startCoords.y);
      el.dispatchEvent(makeTouch("touchstart", [second]));
      await new Promise((resolve) => setTimeout(resolve, 80));

      const dragFresh = findNeedleCols();
      const dragStart = clientCoordsForCol(dragFresh.bufferRow, dragFresh.startCol);
      const dragEnd = clientCoordsForCol(dragFresh.bufferRow, dragFresh.endDragCol);
      const steps = 15;
      for (let step = 1; step <= steps; step += 1) {
        const x = dragStart.x + ((dragEnd.x - dragStart.x) * step) / steps;
        const y = dragStart.y + ((dragEnd.y - dragStart.y) * step) / steps;
        el.dispatchEvent(makeTouch("touchmove", [touchAt(x, y)]));
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      el.dispatchEvent(makeTouch("touchend", []));
    },
    { startNeedle, endNeedle },
  );
}

// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(({}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "terminal acceptance is mobile-webkit only",
  );
});

test("double tap hold drag selects range and shows Copy control", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [
    `prefix ${DOUBLE_TAP_DRAG_START} gap ${DOUBLE_TAP_DRAG_END} suffix\r\n`,
  ]);
  const baseline = await inputFrameCount(page);

  await doubleTapHoldDragTerminalText(page, DOUBLE_TAP_DRAG_START, DOUBLE_TAP_DRAG_END);

  await expect(terminalPanel(page).getByRole("button", { name: "Copy" })).toBeVisible({
    timeout: 10_000,
  });
  const selection = await page.evaluate(() => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as (HTMLElement & { __xterm?: { getSelection: () => string } }) | null;
    return host?.__xterm?.getSelection() ?? "";
  });
  expect(selection).toContain(DOUBLE_TAP_DRAG_START);
  expect(selection).toContain(DOUBLE_TAP_DRAG_END);
  expect(selection.length).toBeGreaterThan(DOUBLE_TAP_DRAG_START.length);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});
