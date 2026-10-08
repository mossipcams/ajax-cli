import { expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
  terminalToolbar,
  terminalInputFrames,
  terminalResizeFrames,
  terminalSocketSummaries,
  waitForTerminalSocket
} from "./fixtures";
import { dispatchViewportEvents } from "./fixtures";
import type { ViewportEventKind } from "./fixtures";


const OPEN = 1;


type TerminalSize = { cols: number; rows: number };


type BandFlushGeometry = {
  bandTop: number;
  bandBottom: number;
  bandHeight: number;
  pinnedTop: number;
  pinnedBottom: number;
  pinnedHeight: number;
  pinnedPosition: string;
  pinnedComputedTop: number;
  pinnedComputedHeight: number;
  pinnedComputedBottom: string;
  keysTop: number;
  keysBottom: number;
  keysHeight: number;
  keyboardOpen: boolean;
  expanded: boolean;
};


export async function activeTaskSocketCount(page: import("@playwright/test").Page) {
  const summaries = await terminalSocketSummaries(page);
  return summaries.filter((socket) => socket.readyState === OPEN).length;
}


export async function gotoTaskRoute(page: import("@playwright/test").Page) {
  await page.goto("/app.html#/t/web%2Ffix-login");
}


export async function clickTerminalSurfaceInterior(page: import("@playwright/test").Page) {
  const surface = terminalSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("terminal surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}


export async function inputFrameCount(page: import("@playwright/test").Page) {
  return (await terminalInputFrames(page)).length;
}


export function hasAdjacentDuplicateSizes(frames: TerminalSize[]): boolean {
  for (let index = 1; index < frames.length; index += 1) {
    const previous = frames[index - 1];
    const current = frames[index];
    if (previous.cols === current.cols && previous.rows === current.rows) return true;
  }
  return false;
}


export const BAND_SETTLE_RESIZE_BUDGET = 4;


export async function waitForResizeFrameCountStable(
  page: import("@playwright/test").Page,
  timeoutMs = 1500,
) {
  await expect
    .poll(
      async () => {
        const before = (await terminalResizeFrames(page)).length;
        await new Promise((resolve) => setTimeout(resolve, 80));
        return (await terminalResizeFrames(page)).length === before;
      },
      { timeout: timeoutMs },
    )
    .toBe(true);
}


export async function openKeyboardBandForResizeTests(page: import("@playwright/test").Page) {
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty(
      "--app-height",
      `${Math.max(0, window.innerHeight - 336)}px`,
    );
  });
  await page.setViewportSize({ width: 390, height: 508 });
  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);
  await waitForResizeFrameCountStable(page);
}


export function sizesEqual(left: TerminalSize | undefined, right: TerminalSize | undefined): boolean {
  return !!left && !!right && left.cols === right.cols && left.rows === right.rows;
}


export const VIEWPORT_EVENT_BURST: ViewportEventKind[] = [
  "resize",
  "orientationchange",
  "visualViewport.resize",
  "resize",
  "visualViewport.resize",
  "orientationchange",
  "resize",
  "visualViewport.resize",
];


export async function openTaskTerminal(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}


export const expandTerminalButton = (page: import("@playwright/test").Page) =>
  terminalSurface(page).getByRole("button", { name: "Expand terminal" });


export const newOutputButton = (page: import("@playwright/test").Page) =>
  page.getByRole("button", { name: "New output ↓" });


export function scrollbackChunk(from: number, count: number): string {
  let out = "";
  for (let i = from; i < from + count; i += 1) {
    out += `row ${i}\r\n`;
  }
  return out;
}


export async function scrollInteractionSurfaceAway(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate((el) => {
    el.scrollTop = Math.max(0, el.scrollTop - 12 * 18);
    el.dispatchEvent(new Event("scroll"));
  });
}


export async function waitForSeedRevealSettled(page: import("@playwright/test").Page) {
  await expect(terminalInteractionSurface(page)).not.toHaveClass(/is-seed-pending/, {
    timeout: 5_000,
  });
}


export async function clickInteractionSurfaceCenter(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}


export async function documentScrollPosition(page: import("@playwright/test").Page) {
  return page.evaluate(() => ({
    windowY: window.scrollY,
    documentY: document.documentElement.scrollTop,
  }));
}


export async function syntheticScrollGestureOnInteractionSurface(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  await surface.dragTo(surface, {
    sourcePosition: { x: box.width / 2, y: box.height * 0.8 },
    targetPosition: { x: box.width / 2, y: box.height * 0.2 },
  });
}


export const COPY_SELECTION_TEXT = "selectable-copy-me";


export const terminalPanel = (page: import("@playwright/test").Page) =>
  page.getByTestId("task-terminal-panel");



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


async function installCopyClipboardFailure(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    document.execCommand = () => false;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("clipboard denied");
        },
        readText: async () => "echo pasted",
      },
    });
  });
}
export async function openTaskTerminalWithCopySpy(page: import("@playwright/test").Page) {
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


export async function openTaskTerminalWithCopyFailure(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await installCopyClipboardFailure(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}


export async function programTerminalSelection(
  page: import("@playwright/test").Page,
  needle: string,
) {
  const selected = await page.evaluate((text) => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as (HTMLElement & { __xterm?: { select: (c: number, r: number, l: number) => void; getSelection: () => string; buffer: { active: { length: number; getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined } } } }) | null;
    const term = host?.__xterm;
    if (!term || typeof term.select !== "function") {
      throw new Error("task terminal xterm instance missing");
    }
    const buffer = term.buffer.active;
    for (let row = 0; row < buffer.length; row += 1) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const str = line.translateToString(true);
      const col = str.indexOf(text);
      if (col >= 0) {
        term.select(col, row, text.length);
        return term.getSelection();
      }
    }
    throw new Error(`terminal text not found: ${text}`);
  }, needle);
  return selected;
}


export async function waitForStableCopyButton(page: import("@playwright/test").Page) {
  const copy = terminalPanel(page).getByRole("button", { name: "Copy" });
  await expect
    .poll(
      async () => {
        if (!(await copy.isVisible().catch(() => false))) return "hidden";
        await new Promise((r) => setTimeout(r, 120));
        return (await copy.isVisible().catch(() => false)) ? "stable" : "detached";
      },
      { timeout: 5_000 },
    )
    .toBe("stable");
  return copy;
}


export const PTY_OUTPUT_CORPUS_CHUNKS: Array<string | number[]> = [
  "ASCII",
  [...new TextEncoder().encode("😀")],
  [...new TextEncoder().encode("e\u0301")],
  [...new TextEncoder().encode("漢")],
  "\x1b[31mRED\x1b[0m\x1b[2K",
  "carriage\rreturn",
  "line\nbreak",
  "crlf\r\nend",
];


export const MULTILINE_UNICODE_CLIPBOARD = "line one\n漢字\ne\u0301";


export const BACK_LEFT = "\x1b[D";

export const BACKSPACE = "\x7f";

export const KEY_REPEAT_HOLD_MS = 650;


export async function holdToolbarButton(
  page: import("@playwright/test").Page,
  name: string,
  holdMs: number,
) {
  const button = terminalToolbar(page).getByRole("button", { name });
  const box = await button.boundingBox();
  if (!box) throw new Error(`toolbar button ${name} box missing`);
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await new Promise((resolve) => setTimeout(resolve, holdMs));
  await page.mouse.up();
}


export async function tapToolbarButton(page: import("@playwright/test").Page, name: string) {
  const button = terminalToolbar(page).getByRole("button", { name });
  const box = await button.boundingBox();
  if (!box) throw new Error(`toolbar button ${name} box missing`);
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}


export async function settleNoNewFrames(
  page: import("@playwright/test").Page,
  expectedCount: number,
) {
  await new Promise((resolve) => setTimeout(resolve, 200));
  await expect.poll(async () => inputFrameCount(page)).toBe(expectedCount);
}


export async function simulateKeyboardBand(
  page: import("@playwright/test").Page,
  band: { top?: number; height?: number } = {},
) {
  const top = band.top ?? 40;
  const height = band.height ?? 460;
  await page.evaluate(
    ({ top: appTop, height: appHeight }) => {
      document.documentElement.classList.add("keyboard-open");
      document.documentElement.style.setProperty("--app-height", `${appHeight}px`);
      document.documentElement.style.setProperty("--app-top", `${appTop}px`);
    },
    { top, height },
  );
}


export async function clearKeyboardBand(page: import("@playwright/test").Page) {
  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
    document.documentElement.style.removeProperty("--app-top");
  });
}


export async function visibleAppBand(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const top = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    );
    const height = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
    );
    return { top, height, bottom: top + height };
  });
}


export async function readBandFlushGeometry(
  page: import("@playwright/test").Page,
  pinnedSelector: string,
): Promise<BandFlushGeometry | null> {
  return page.evaluate((selector) => {
    const pinned = document.querySelector<HTMLElement>(selector);
    const keys = document.querySelector<HTMLElement>('[data-testid="terminal-bottom-controls"]');
    if (!pinned || !keys) return null;
    const top = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    );
    const height = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
    );
    const pinnedBox = pinned.getBoundingClientRect();
    const keysBox = keys.getBoundingClientRect();
    const pinnedStyle = getComputedStyle(pinned);
    return {
      bandTop: top,
      bandBottom: top + height,
      bandHeight: height,
      pinnedTop: pinnedBox.top,
      pinnedBottom: pinnedBox.bottom,
      pinnedHeight: pinnedBox.height,
      pinnedPosition: pinnedStyle.position,
      pinnedComputedTop: Number.parseFloat(pinnedStyle.top) || 0,
      pinnedComputedHeight: Number.parseFloat(pinnedStyle.height) || 0,
      pinnedComputedBottom: pinnedStyle.bottom,
      keysTop: keysBox.top,
      keysBottom: keysBox.bottom,
      keysHeight: keysBox.height,
      keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      expanded: document.documentElement.classList.contains("terminal-expanded"),
    };
  }, pinnedSelector);
}


export function expectFlushToBand(
  geometry: BandFlushGeometry | null,
  options: { expanded: boolean; position?: string } = { expanded: false },
) {
  expect(geometry).not.toBeNull();
  const g = geometry!;
  expect(g.keyboardOpen).toBe(true);
  expect(g.expanded).toBe(options.expanded);
  expect(g.pinnedPosition).toBe(options.position ?? "fixed");
  expect(g.pinnedComputedTop).toBeCloseTo(g.bandTop, 0);
  expect(g.pinnedComputedHeight).toBeCloseTo(g.bandHeight, 0);
  expect(Math.abs(g.pinnedComputedHeight - g.bandHeight)).toBeLessThanOrEqual(1);
  expect(g.pinnedTop).toBeCloseTo(g.bandTop, 0);
  expect(g.pinnedBottom).toBeCloseTo(g.bandBottom, 0);
  expect(g.pinnedHeight).toBeCloseTo(g.bandHeight, 0);
  expect(g.keysBottom).toBeCloseTo(g.bandBottom, 0);
  expect(Math.abs(g.keysBottom - g.pinnedBottom)).toBeLessThanOrEqual(1);
  expect(g.keysTop).toBeGreaterThanOrEqual(g.bandTop - 1);
  expect(g.keysBottom).toBeLessThanOrEqual(g.bandBottom + 1);
  expect(g.keysHeight).toBeGreaterThan(0);
  expect(g.keysTop).toBeLessThan(g.keysBottom);
}


export function boxesIntersect(
  box: { y: number; height: number },
  band: { top: number; bottom: number },
): boolean {
  const boxBottom = box.y + box.height;
  return box.y < band.bottom && boxBottom > band.top;
}


export async function chromeDisplayState(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const cockpit = document.querySelector(".cockpit-chrome");
    const bottomNav = document.querySelector(".bottom-nav");
    const detailHeader = document.querySelector(".task-detail .detail-header");
    const interactPanel = document.querySelector(".task-detail .interact-panel");
    return {
      cockpit: cockpit ? getComputedStyle(cockpit).display : null,
      bottomNav: bottomNav ? getComputedStyle(bottomNav).display : null,
      detailHeader: detailHeader ? getComputedStyle(detailHeader).display : null,
      interactPanel: interactPanel ? getComputedStyle(interactPanel).display : null,
    };
  });
}


export const COMPACT_KEY_MIN_PX = 32;

export const COMPACT_KEY_MAX_PX = 40;

export const PRIMARY_TOUCH_MIN_PX = 44;
